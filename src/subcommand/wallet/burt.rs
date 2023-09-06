use super::*;
use crate::index::{ConstructTransaction, MysqlDatabase, TransactionOutputArray};
use bitcoin::blockdata::{script, witness::Witness};
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::psbt::Psbt;
use bitcoin::{AddressType, PackedLockTime};

#[derive(Debug, Parser)]
pub struct Burt {
  #[clap(long, help = "Send inscription from <DESTINATION>.")]
  pub destination: Address,
  #[clap(long, help = "The burt txs that needs to be burt.")]
  pub burt_txs: Vec<Txid>,
  #[clap(long, help = "Use fee rate of <FEE_RATE> sats/vB")]
  pub fee_rate: FeeRate,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
  pub transaction: String,
  pub commit_custom: Vec<String>,
  pub network_fee: u64,
  pub service_fee: u64,
  pub commit_vsize: u64,
  pub commit_fee: u64,
  pub min_fee_rate: f64,
}

impl Burt {
  pub fn build(
    self,
    options: Options,
    service_address: Option<Address>,
    _service_fee: Option<Amount>,
    _mysql: Option<Arc<MysqlDatabase>>,
  ) -> Result<Output> {
    if !self.burt_txs.is_empty() {
      bail!("Burt txs is empty");
    }

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

    log::info!("Open index...");
    let index = Index::read_open(&options)?;
    // index.update()?;

    log::info!("Get utxo...");
    let (burt_utxo, burt_txs) = index.get_txs(&self.burt_txs)?;

    let output = vec![TxOut {
      script_pubkey: self.destination.script_pubkey(),
      value: 0,
    }];
    let (mut update_burt_tx, network_fee, last_output_amount) =
      Self::build_burt_transaction(self.fee_rate, &burt_txs, output);
    let commit_vsize = update_burt_tx.vsize() as u64;

    let input_amount = Self::get_amount(&update_burt_tx, &burt_utxo)?;
    if input_amount <= network_fee {
      bail!("Input amount less than network fee");
    }
    update_burt_tx.output[0].value = input_amount - network_fee;
    for input in &mut update_burt_tx.input {
      input.witness = Witness::new();
    }

    let unsigned_transaction_psbt = Self::get_psbt(&update_burt_tx, &burt_utxo, &self.destination)?;
    let unsigned_commit_custom = Self::get_custom(&unsigned_transaction_psbt);

    log::info!("Build burt success");

    let min_fee_rate = (last_output_amount as f64) / (commit_vsize as f64);

    Ok(Output {
      transaction: serialize_hex(&unsigned_transaction_psbt),
      commit_custom: unsigned_commit_custom,
      network_fee,
      service_fee: 0,
      commit_vsize,
      commit_fee: network_fee,
      min_fee_rate,
    })
  }

  pub fn run(self, options: Options) -> Result {
    print_json(self.build(options, None, None, None)?)?;
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
    destination: &Address,
  ) -> Result<Psbt> {
    let mut tx_psbt = Psbt::from_unsigned_tx(tx.clone())?;
    for i in 0..tx_psbt.unsigned_tx.input.len() {
      tx_psbt.inputs[i].witness_utxo = Some(TxOut {
        value: utxos
          .get(&tx_psbt.unsigned_tx.input[i].previous_output)
          .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?
          .to_sat(),
        script_pubkey: destination.script_pubkey(),
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

  fn build_burt_transaction(
    fee_rate: FeeRate,
    burt_txs: &Vec<Transaction>,
    output: Vec<TxOut>,
  ) -> (Transaction, u64, u64) {
    let mut input = vec![];
    let mut last_output_amount = 0;

    for burt_tx in burt_txs {
      input.append(&mut burt_tx.input.clone());
      for o in &burt_tx.output {
        last_output_amount += o.value;
      }
    }

    let update_burt_tx = Transaction {
      input,
      output,
      lock_time: PackedLockTime::ZERO,
      version: 1,
    };

    let fee = fee_rate.fee(update_burt_tx.vsize());
    (update_burt_tx, fee.to_sat(), last_output_amount)
  }
}
