use crate::index::{ConstructTransaction, MysqlDatabase, TransactionOutputArray};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::psbt::Psbt;
use bitcoincore_rpc::RawTx;
use {
  super::*,
  bitcoin::{
    blockdata::{opcodes, script},
    policy::MAX_STANDARD_TX_WEIGHT,
    schnorr::{TapTweak, TweakedKeyPair, TweakedPublicKey, UntweakedKeyPair},
    secp256k1::{
      self, constants::SCHNORR_SIGNATURE_SIZE, rand, schnorr::Signature, Secp256k1, XOnlyPublicKey,
    },
    util::sighash::{Prevouts, SighashCache},
    util::taproot::{ControlBlock, LeafVersion, TapLeafHash, TaprootBuilder},
    PackedLockTime, SchnorrSighashType, Witness,
  },
  std::collections::BTreeSet,
};

#[derive(Debug, Serialize)]
pub struct Output {
  pub inscription: Vec<InscriptionId>,
  pub commit: String,
  pub commit_custom: Vec<String>,
  pub reveal: Vec<String>,
  pub service_fee: u64,
  pub satpoint_fee: u64,
  pub network_fee: u64,
}

#[derive(Debug, Parser)]
pub struct Mint {
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB")]
  pub fee_rate: FeeRate,
  #[clap(long, help = "Send inscription to <DESTINATION>.")]
  pub destination: Option<Address>,
  #[clap(long, help = "Send inscription from <SOURCE>.")]
  pub source: Address,
  #[clap(long, help = "Content type of mint, '.txt'.")]
  pub extension: Option<String>,
  #[clap(long, help = "Content of mint.")]
  pub content: String,
  #[clap(long, help = "Repeat count of mint.")]
  pub repeat: Option<u64>,
}

impl Mint {
  pub const SERVICE_FEE: Amount = Amount::from_sat(3000);

  pub fn build(
    self,
    options: Options,
    service_address: Option<Address>,
    service_fee: Option<Amount>,
    mysql: Option<Arc<MysqlDatabase>>,
  ) -> Result<Output> {
    let repeat: u64 = self.repeat.unwrap_or(1);
    let extension = "data.".to_owned() + &self.extension.unwrap_or(".txt".to_owned());

    let inscription = Inscription::from_content(options.chain(), &extension, self.content)?;

    log::info!("Open index...");
    let index = Index::read_open(&options)?;
    // index.update()?;

    let source = self.source;
    let reveal_tx_destination = self.destination.unwrap_or_else(|| source.clone());

    if !source.is_valid_for_network(options.chain().network()) {
      bail!("Address `{}` is not valid for {}", source, options.chain());
    }
    if !reveal_tx_destination.is_valid_for_network(options.chain().network()) {
      bail!(
        "Address `{}` is not valid for {}",
        reveal_tx_destination,
        options.chain()
      );
    }

    let service_address = service_address.unwrap_or(source.clone());

    log::info!("Get utxo...");
    let query_address = &format!("{}", source);
    let utxos = index.get_unspent_outputs_by_mempool(query_address)?;

    let inscriptions = if let Some(mysql) = mysql {
      log::info!("Get inscriptions by mysql...");
      mysql.get_inscription_by_address(query_address)?
    } else {
      log::info!("Get inscriptions by redb...");
      index.get_inscriptions(None)?
    };

    let commit_tx_change = [source.clone(), source.clone()];

    let (
      unsigned_commit_tx,
      reveal_txs,
      _recovery_key_pair,
      service_fee,
      satpoint_fee,
      network_fee,
    ) = Mint::create_inscription_transactions(
      None,
      inscription,
      inscriptions,
      options.chain().network(),
      utxos.clone(),
      commit_tx_change,
      reveal_tx_destination,
      self.fee_rate,
      self.fee_rate,
      false,
      service_address,
      usize::try_from(repeat)?,
      service_fee.unwrap_or(Self::SERVICE_FEE),
    )?;

    let network_fee = Self::calculate_fee(&unsigned_commit_tx, &utxos) + network_fee;

    let unsigned_commit_psbt = Self::get_psbt(&unsigned_commit_tx, &utxos, &source)?;
    let unsigned_commit_custom = Self::get_custom(&unsigned_commit_psbt);

    let output = Output {
      commit: serialize_hex(&unsigned_commit_psbt),
      commit_custom: unsigned_commit_custom,
      reveal: reveal_txs
        .clone()
        .into_iter()
        .map(|tx| tx.raw_hex())
        .collect(),
      inscription: reveal_txs.into_iter().map(|tx| tx.txid().into()).collect(),
      service_fee,
      satpoint_fee,
      network_fee,
    };
    log::info!("Build mint success");
    Ok(output)
  }

  pub fn run(self, options: Options) -> Result {
    print_json(self.build(options, None, Some(Self::SERVICE_FEE), None)?)?;
    Ok(())
  }

  fn get_psbt(
    tx: &Transaction,
    utxos: &BTreeMap<OutPoint, Amount>,
    source: &Address,
  ) -> Result<Psbt> {
    let mut tx_psbt = Psbt::from_unsigned_tx(tx.clone())?;
    for i in 0..tx_psbt.unsigned_tx.input.len() {
      tx_psbt.inputs[i].witness_utxo = Some(TxOut {
        value: utxos
          .get(&tx_psbt.unsigned_tx.input[i].previous_output)
          .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
          .to_sat(),
        script_pubkey: source.script_pubkey(),
      });
    }
    Ok(tx_psbt)
  }

  fn get_custom(tx: &Psbt) -> Vec<String> {
    let unsigned_commit_custom = ConstructTransaction {
      pre_outputs: TransactionOutputArray {
        outputs: tx
          .inputs
          .iter()
          .map(|v| v.witness_utxo.clone().expect("Must has input"))
          .collect(),
      },
      cur_transaction: tx.unsigned_tx.clone(),
    };

    let mut result: Vec<String> = vec![serialize_hex(&unsigned_commit_custom)];
    for v in tx.unsigned_tx.input.iter() {
      result.push(format!("{}", v.previous_output.txid));
      result.push(v.previous_output.vout.to_string())
    }

    result
  }

  fn calculate_fee(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> u64 {
    tx.input
      .iter()
      .map(|txin| utxos.get(&txin.previous_output).unwrap().to_sat())
      .sum::<u64>()
      .checked_sub(tx.output.iter().map(|txout| txout.value).sum::<u64>())
      .unwrap()
  }

  fn create_inscription_transactions(
    satpoint: Option<SatPoint>,
    inscription: Inscription,
    inscriptions: BTreeMap<SatPoint, InscriptionId>,
    network: Network,
    utxos: BTreeMap<OutPoint, Amount>,
    change: [Address; 2],
    destination: Address,
    commit_fee_rate: FeeRate,
    reveal_fee_rate: FeeRate,
    no_limit: bool,
    service_address: Address,
    repeat: usize,
    service_fee: Amount,
  ) -> Result<(Transaction, Vec<Transaction>, TweakedKeyPair, u64, u64, u64)> {
    let satpoint = if let Some(satpoint) = satpoint {
      satpoint
    } else {
      let inscribed_utxos = inscriptions
        .keys()
        .map(|satpoint| satpoint.outpoint)
        .collect::<BTreeSet<OutPoint>>();

      utxos
        .keys()
        .find(|outpoint| !inscribed_utxos.contains(outpoint))
        .map(|outpoint| SatPoint {
          outpoint: *outpoint,
          offset: 0,
        })
        .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
    };

    for (inscribed_satpoint, inscription_id) in &inscriptions {
      if inscribed_satpoint == &satpoint {
        return Err(anyhow!("sat at {} already inscribed", satpoint));
      }

      if inscribed_satpoint.outpoint == satpoint.outpoint {
        return Err(anyhow!(
          "utxo {} already inscribed with inscription {inscription_id} on sat {inscribed_satpoint}",
          satpoint.outpoint,
        ));
      }
    }

    let secp256k1 = Secp256k1::new();
    let key_pair = UntweakedKeyPair::new(&secp256k1, &mut rand::thread_rng());
    let (public_key, _parity) = XOnlyPublicKey::from_keypair(&key_pair);

    let reveal_script = inscription.append_reveal_script(
      script::Builder::new()
        .push_slice(&public_key.serialize())
        .push_opcode(opcodes::all::OP_CHECKSIG),
    );

    let taproot_spend_info = TaprootBuilder::new()
      .add_leaf(0, reveal_script.clone())
      .expect("adding leaf should work")
      .finalize(&secp256k1, public_key)
      .expect("finalizing taproot builder should work");

    let control_block = taproot_spend_info
      .control_block(&(reveal_script.clone(), LeafVersion::TapScript))
      .expect("should compute control block");

    let commit_tx_address = Address::p2tr_tweaked(taproot_spend_info.output_key(), network);

    let mut reveal_fees: Vec<Amount> = vec![];
    let mut next_remain_fees: Vec<Amount> = vec![];

    for i in (0..repeat).rev() {
      let reveal_output = if i == 0 && repeat == 1 {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: 0,
          },
          TxOut {
            script_pubkey: service_address.script_pubkey(),
            value: 0,
          },
        ]
      } else if i == 0 && repeat > 1 {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: 0,
          },
          TxOut {
            script_pubkey: commit_tx_address.script_pubkey(),
            value: 0,
          },
          TxOut {
            script_pubkey: service_address.script_pubkey(),
            value: 0,
          },
        ]
      } else if i + 1 < repeat {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: 0,
          },
          TxOut {
            script_pubkey: commit_tx_address.script_pubkey(),
            value: 0,
          },
        ]
      } else {
        vec![TxOut {
          script_pubkey: destination.script_pubkey(),
          value: 0,
        }]
      };
      let (_, reveal_fee) = Self::build_reveal_transaction(
        &control_block,
        reveal_fee_rate,
        OutPoint::null(),
        reveal_output,
        &reveal_script,
      );
      if i + 1 < repeat {
        next_remain_fees.push(
          (*reveal_fees.last().unwrap())
            + (*next_remain_fees.last().unwrap_or(&Amount::ZERO))
            + TransactionBuilder::TARGET_POSTAGE,
        );
      }
      reveal_fees.push(reveal_fee);
    }
    reveal_fees.reverse();
    next_remain_fees.reverse();

    let unsigned_commit_tx = TransactionBuilder::build_transaction_with_value(
      satpoint,
      inscriptions,
      utxos,
      commit_tx_address.clone(),
      change,
      commit_fee_rate,
      reveal_fees[0]
        + TransactionBuilder::TARGET_POSTAGE
        + *next_remain_fees.get(0).unwrap_or(&Amount::ZERO)
        + (service_fee * (repeat as u64)),
    )?;

    let (vout, output) = unsigned_commit_tx
      .output
      .iter()
      .enumerate()
      .find(|(_vout, output)| output.script_pubkey == commit_tx_address.script_pubkey())
      .expect("should find sat commit/inscription output");

    let mut reveal_txs: Vec<Transaction> = vec![];

    let service_fee = (service_fee * (repeat as u64)).to_sat();
    let satpoint_fee = (TransactionBuilder::TARGET_POSTAGE * (repeat as u64)).to_sat();
    let network_fee = reveal_fees.into_iter().sum::<Amount>().to_sat();
    for i in 0..repeat {
      let reveal_output = if i == 0 && repeat == 1 {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: TransactionBuilder::TARGET_POSTAGE.to_sat(),
          },
          TxOut {
            script_pubkey: service_address.script_pubkey(),
            value: service_fee,
          },
        ]
      } else if i == 0 && repeat > 1 {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: TransactionBuilder::TARGET_POSTAGE.to_sat(),
          },
          TxOut {
            script_pubkey: commit_tx_address.script_pubkey(),
            value: next_remain_fees[i].to_sat(),
          },
          TxOut {
            script_pubkey: service_address.script_pubkey(),
            value: service_fee,
          },
        ]
      } else if i + 1 < repeat {
        vec![
          TxOut {
            script_pubkey: destination.script_pubkey(),
            value: TransactionBuilder::TARGET_POSTAGE.to_sat(),
          },
          TxOut {
            script_pubkey: commit_tx_address.script_pubkey(),
            value: next_remain_fees[i].to_sat(),
          },
        ]
      } else {
        vec![TxOut {
          script_pubkey: destination.script_pubkey(),
          value: TransactionBuilder::TARGET_POSTAGE.to_sat(),
        }]
      };

      let (txid, vout) = if i == 0 {
        (unsigned_commit_tx.txid(), vout.try_into().unwrap())
      } else {
        (reveal_txs[i - 1].txid(), 1)
      };

      let (mut reveal_tx, _fee) = Self::build_reveal_transaction(
        &control_block,
        reveal_fee_rate,
        OutPoint { txid, vout },
        reveal_output,
        &reveal_script,
      );

      if reveal_tx.output[0].value < reveal_tx.output[0].script_pubkey.dust_value().to_sat() {
        bail!("commit transaction output would be dust");
      }

      let mut sighash_cache = SighashCache::new(&mut reveal_tx);

      let prevout = if i == 0 {
        output
      } else {
        &reveal_txs[i - 1].output[1]
      };

      let signature_hash = sighash_cache
        .taproot_script_spend_signature_hash(
          0,
          &Prevouts::All(&[prevout]),
          TapLeafHash::from_script(&reveal_script, LeafVersion::TapScript),
          SchnorrSighashType::Default,
        )
        .expect("signature hash should compute");

      let signature = secp256k1.sign_schnorr(
        &secp256k1::Message::from_slice(signature_hash.as_inner())
          .expect("should be cryptographically secure hash"),
        &key_pair,
      );

      let witness = sighash_cache
        .witness_mut(0)
        .expect("getting mutable witness reference should work");
      witness.push(signature.as_ref());
      witness.push(reveal_script.clone());
      witness.push(&control_block.serialize());

      let reveal_weight = reveal_tx.weight();

      if !no_limit && reveal_weight > MAX_STANDARD_TX_WEIGHT.try_into().unwrap() {
        bail!(
        "reveal transaction weight greater than {MAX_STANDARD_TX_WEIGHT} (MAX_STANDARD_TX_WEIGHT): {reveal_weight}"
      );
      }

      reveal_txs.push(reveal_tx);
    }

    let recovery_key_pair = key_pair.tap_tweak(&secp256k1, taproot_spend_info.merkle_root());

    let (x_only_pub_key, _parity) = recovery_key_pair.to_inner().x_only_public_key();
    assert_eq!(
      Address::p2tr_tweaked(
        TweakedPublicKey::dangerous_assume_tweaked(x_only_pub_key),
        network,
      ),
      commit_tx_address
    );

    Ok((
      unsigned_commit_tx,
      reveal_txs,
      recovery_key_pair,
      service_fee,
      satpoint_fee,
      network_fee,
    ))
  }

  fn build_reveal_transaction(
    control_block: &ControlBlock,
    fee_rate: FeeRate,
    input: OutPoint,
    output: Vec<TxOut>,
    script: &Script,
  ) -> (Transaction, Amount) {
    let reveal_tx = Transaction {
      input: vec![TxIn {
        previous_output: input,
        script_sig: script::Builder::new().into_script(),
        witness: Witness::new(),
        sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
      }],
      output,
      lock_time: PackedLockTime::ZERO,
      version: 1,
    };

    let fee = {
      let mut reveal_tx = reveal_tx.clone();

      reveal_tx.input[0].witness.push(
        Signature::from_slice(&[0; SCHNORR_SIGNATURE_SIZE])
          .unwrap()
          .as_ref(),
      );
      reveal_tx.input[0].witness.push(script);
      reveal_tx.input[0].witness.push(&control_block.serialize());

      fee_rate.fee(reveal_tx.vsize())
    };

    (reveal_tx, fee)
  }
}
