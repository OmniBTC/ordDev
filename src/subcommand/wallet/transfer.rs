use super::*;
use crate::index::{ConstructTransaction, MysqlDatabase, TransactionOutputArray};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::psbt::Psbt;
use bitcoin::AddressType;
use std::collections::BTreeSet;

#[derive(Debug, Parser)]
pub struct Transfer {
  #[clap(long, help = "Send inscription to <DESTINATION>.")]
  pub destination: Address,
  #[clap(long, help = "Send inscription from <SOURCE>.")]
  pub source: Address,
  pub outgoing: Outgoing,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB")]
  pub fee_rate: FeeRate,
  #[clap(long, help = "Allow <OP_RETURN>.")]
  pub op_return: Option<String>,
  #[clap(long, help = "Whether to transfer brc20.")]
  pub brc20_transfer: Option<bool>,
  pub addition_outgoing: Vec<Outgoing>,
  #[clap(long, help = "Addition Fee for destination address.")]
  pub addition_fee: Amount,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
  pub transaction: String,
  pub commit_custom: Vec<String>,
  pub network_fee: u64,
}

impl Transfer {
  pub fn build(self, options: Options, mysql: Option<Arc<MysqlDatabase>>) -> Result<Output> {
    if !self
      .destination
      .is_valid_for_network(options.chain().network())
    {
      bail!(
        "Address `{}` is not valid for {}",
        self.destination,
        options.chain()
      );
    }
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

    let brc20_transfer = self.brc20_transfer.unwrap_or(false);
    log::info!("Open index...");
    let index = Index::read_open(&options)?;
    // index.update()?;

    log::info!("Get utxo...");
    let query_address = &format!("{}", self.source);

    let inscriptions = if let Some(mysql) = mysql {
      log::info!("Get inscriptions by mysql...");
      mysql.get_inscription_by_address(query_address)?
    } else {
      log::info!("Get inscriptions by redb...");
      index.get_inscriptions(None)?
    };

    let change = [self.source.clone(), self.source.clone()];

    let (satpoints, amount, unspent_outputs) = match self.outgoing {
      Outgoing::SatPoint(satpoint) => {
        for inscription_satpoint in inscriptions.keys() {
          if satpoint == *inscription_satpoint {
            bail!("inscriptions must be sent by inscription ID");
          }
        }

        let mut satpoints = vec![satpoint];

        for item in &self.addition_outgoing {
          if let Outgoing::SatPoint(satpoint) = *item {
            for inscription_satpoint in inscriptions.keys() {
              if satpoint == *inscription_satpoint {
                bail!("inscriptions must be sent by inscription ID");
              }
            }
            satpoints.push(satpoint)
          } else {
            bail!("Addition outgoing must be satpoint");
          }
        }

        (
          satpoints,
          TransactionBuilder::TARGET_POSTAGE * (1 + (self.addition_outgoing.len() as u64))
            + self.addition_fee,
          index.get_unspent_outputs_by_mempool_v1(query_address, BTreeMap::new())?,
        )
      }
      Outgoing::InscriptionId(id) => {
        if brc20_transfer {
          let mut remain_outpoint = BTreeMap::new();
          remain_outpoint.insert(
            OutPoint {
              txid: id.txid,
              vout: 0,
            },
            true,
          );

          let satpoint = SatPoint {
            outpoint: OutPoint {
              txid: id.txid,
              vout: 0,
            },
            offset: 0,
          };
          let mut satpoints = vec![satpoint];

          for item in &self.addition_outgoing {
            if let Outgoing::InscriptionId(id) = *item {
              remain_outpoint.insert(
                OutPoint {
                  txid: id.txid,
                  vout: 0,
                },
                true,
              );
              let satpoint = SatPoint {
                outpoint: OutPoint {
                  txid: id.txid,
                  vout: 0,
                },
                offset: 0,
              };
              satpoints.push(satpoint)
            } else {
              bail!("Addition outgoing must be satpoint");
            }
          }

          (
            satpoints,
            TransactionBuilder::TARGET_POSTAGE * (1 + (self.addition_outgoing.len() as u64))
              + self.addition_fee,
            index.get_unspent_outputs_by_mempool_v1(query_address, remain_outpoint)?,
          )
        } else {
          let satpoint = index
            .get_inscription_satpoint_by_id(id)?
            .ok_or_else(|| anyhow!("Inscription {id} not found"))?;
          let mut satpoints = vec![satpoint];

          for item in &self.addition_outgoing {
            if let Outgoing::InscriptionId(id) = *item {
              let satpoint = index
                .get_inscription_satpoint_by_id(id)?
                .ok_or_else(|| anyhow!("Inscription {id} not found"))?;
              satpoints.push(satpoint)
            } else {
              bail!("Addition outgoing must be satpoint");
            }
          }

          (
            satpoints,
            TransactionBuilder::TARGET_POSTAGE * (1 + (self.addition_outgoing.len() as u64)),
            index.get_unspent_outputs_by_mempool_v1(query_address, BTreeMap::new())?,
          )
        }
      }
      Outgoing::Amount(amount) => {
        let inscribed_utxos = inscriptions
          .keys()
          .map(|satpoint| satpoint.outpoint)
          .collect::<BTreeSet<OutPoint>>();
        let unspent_outputs =
          index.get_unspent_outputs_by_mempool_v1(query_address, BTreeMap::new())?;
        let satpoint = unspent_outputs
          .keys()
          .find(|outpoint| !inscribed_utxos.contains(outpoint))
          .map(|outpoint| SatPoint {
            outpoint: *outpoint,
            offset: 0,
          })
          .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?;
        (vec![satpoint], amount + self.addition_fee, unspent_outputs)
      }
    };

    let unsigned_transaction = if let Some(op_return) = self.op_return {
      TransactionBuilder::build_multi_outgoing_with_op_return(
        address_type,
        satpoints,
        inscriptions,
        unspent_outputs.clone(),
        self.destination,
        change,
        self.fee_rate,
        amount,
        op_return,
      )?
    } else {
      TransactionBuilder::build_multi_outgoing_with_value(
        address_type,
        satpoints,
        inscriptions,
        unspent_outputs.clone(),
        self.destination,
        change,
        self.fee_rate,
        amount,
      )?
    };

    let network_fee = Self::calculate_fee(&unsigned_transaction, &unspent_outputs);

    let unsigned_transaction_psbt =
      Self::get_psbt(&unsigned_transaction, &unspent_outputs, &self.source)?;
    let unsigned_commit_custom = Self::get_custom(&unsigned_transaction_psbt);

    log::info!("Build transfer success");

    Ok(Output {
      transaction: serialize_hex(&unsigned_transaction_psbt),
      commit_custom: unsigned_commit_custom,
      network_fee,
    })
  }

  pub fn run(self, options: Options) -> Result {
    print_json(self.build(options, None)?)?;
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
}
