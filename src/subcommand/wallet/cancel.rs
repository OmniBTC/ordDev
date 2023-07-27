use super::*;
use crate::index::{ConstructTransaction, MysqlDatabase, TransactionOutputArray};
use bitcoin::blockdata::{script, witness::Witness};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::psbt::Psbt;
use bitcoin::{AddressType, PackedLockTime};

#[derive(Debug, Parser)]
pub struct Cancel {
  #[clap(long, help = "Send inscription from <SOURCE>.")]
  pub source: Address,
  #[clap(long, help = "The inputs that needs to be canceled.")]
  pub inputs: Vec<OutPoint>,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB")]
  pub fee_rate: FeeRate,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
  pub transaction: String,
  pub commit_custom: Vec<String>,
  pub network_fee: u64,
  pub commit_vsize: u64,
  pub commit_fee: u64,
}

impl Cancel {
  pub fn build(self, options: Options, _mysql: Option<Arc<MysqlDatabase>>) -> Result<Output> {
    if !self.source.is_valid_for_network(options.chain().network()) {
      bail!(
        "Address `{}` is not valid for {}",
        self.source,
        options.chain()
      );
    }

    // check address types, only support p2tr and p2wpkh
    let address_type = if let Some(address_type) = self.source.address_type() {
      if (address_type == AddressType::P2tr) || (address_type == AddressType::P2wpkh) {
        address_type
      } else {
        bail!(
          "Address type `{}` is not valid, only support p2tr and p2wpkh",
          address_type
        );
      }
    } else {
      bail!(
        "Address `{}` is not valid for {}",
        self.source,
        options.chain()
      );
    };

    log::info!("Open index...");
    let index = Index::read_open(&options)?;
    // index.update()?;

    log::info!("Get utxo...");
    let unspent_outputs = index.get_unspent_outputs_by_outpoints(&self.inputs)?;

    let output = vec![TxOut {
      script_pubkey: self.source.script_pubkey(),
      value: 0,
    }];
    let (mut cancel_tx, network_fee) =
      Self::build_cancel_transaction(self.fee_rate, self.inputs, output, address_type);
    let commit_vsize = cancel_tx.vsize() as u64;

    let input_amount = Self::get_amount(&cancel_tx, &unspent_outputs)?;
    if input_amount <= network_fee {
      bail!("Input amount less than network fee");
    }
    cancel_tx.output[0].value = input_amount - network_fee;
    for input in &mut cancel_tx.input {
      input.witness = Witness::new();
    }

    let unsigned_transaction_psbt = Self::get_psbt(&cancel_tx, &unspent_outputs, &self.source)?;
    let unsigned_commit_custom = Self::get_custom(&unsigned_transaction_psbt);

    log::info!("Build cancel success");

    Ok(Output {
      transaction: serialize_hex(&unsigned_transaction_psbt),
      commit_custom: unsigned_commit_custom,
      network_fee,
      commit_vsize,
      commit_fee: network_fee,
    })
  }

  pub fn run(self, options: Options) -> Result {
    print_json(self.build(options, None)?)?;
    Ok(())
  }

  fn get_amount(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> Result<u64> {
    let mut amount = 0;
    for i in 0..tx.input.len() {
      amount += utxos
        .get(&tx.input[i].previous_output)
        .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
        .to_sat();
    }
    Ok(amount)
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

  fn build_cancel_transaction(
    fee_rate: FeeRate,
    input: Vec<OutPoint>,
    output: Vec<TxOut>,
    input_type: AddressType,
  ) -> (Transaction, u64) {
    let witness_size = if input_type == AddressType::P2tr {
      TransactionBuilder::SCHNORR_SIGNATURE_SIZE
    } else {
      TransactionBuilder::P2WPKH_WINETSS_SIZE
    };

    let cancel_tx = Transaction {
      input: input
        .iter()
        .map(|item| TxIn {
          previous_output: *item,
          script_sig: script::Builder::new().into_script(),
          witness: Witness::from_vec(vec![vec![0; witness_size]]),
          sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
        })
        .collect(),
      output,
      lock_time: PackedLockTime::ZERO,
      version: 1,
    };

    let fee = fee_rate.fee(cancel_tx.vsize());
    (cancel_tx, fee.to_sat())
  }
}
