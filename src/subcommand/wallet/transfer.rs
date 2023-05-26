use super::*;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::psbt::Psbt;
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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Output {
  pub transaction: String,
  pub network_fee: u64,
}

impl Transfer {
  pub fn build(self, options: Options) -> Result<Output> {
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

    let index = Index::open(&options)?;
    // index.update()?;

    let unspent_outputs = index.get_unspent_outputs_by_mempool(&format!("{}", self.source))?;

    let inscriptions = index.get_inscriptions(None)?;

    let change = [self.source.clone(), self.source.clone()];

    let (satpoint, amount) = match self.outgoing {
      Outgoing::SatPoint(satpoint) => {
        for inscription_satpoint in inscriptions.keys() {
          if satpoint == *inscription_satpoint {
            bail!("inscriptions must be sent by inscription ID");
          }
        }
        (satpoint, TransactionBuilder::TARGET_POSTAGE)
      }
      Outgoing::InscriptionId(id) => (
        index
          .get_inscription_satpoint_by_id(id)?
          .ok_or_else(|| anyhow!("Inscription {id} not found"))?,
        TransactionBuilder::TARGET_POSTAGE,
      ),
      Outgoing::Amount(amount) => {
        let inscribed_utxos = inscriptions
          .keys()
          .map(|satpoint| satpoint.outpoint)
          .collect::<BTreeSet<OutPoint>>();

        let satpoint = unspent_outputs
          .keys()
          .find(|outpoint| !inscribed_utxos.contains(outpoint))
          .map(|outpoint| SatPoint {
            outpoint: *outpoint,
            offset: 0,
          })
          .ok_or_else(|| anyhow!("wallet contains no cardinal utxos"))?;
        (satpoint, amount)
      }
    };

    let unsigned_transaction = TransactionBuilder::build_transaction_with_value(
      satpoint,
      inscriptions,
      unspent_outputs.clone(),
      self.destination,
      change,
      self.fee_rate,
      amount,
    )?;

    let network_fee = Self::calculate_fee(&unsigned_transaction, &unspent_outputs);

    let unsigned_transaction_psbt =
      Self::get_psbt(&unsigned_transaction, &unspent_outputs, &self.source)?;

    Ok(Output {
      transaction: serialize_hex(&unsigned_transaction_psbt),
      network_fee,
    })
  }

  pub fn run(self, options: Options) -> Result {
    print_json(self.build(options)?)?;
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

  fn calculate_fee(tx: &Transaction, utxos: &BTreeMap<OutPoint, Amount>) -> u64 {
    tx.input
      .iter()
      .map(|txin| utxos.get(&txin.previous_output).unwrap().to_sat())
      .sum::<u64>()
      .checked_sub(tx.output.iter().map(|txout| txout.value).sum::<u64>())
      .unwrap()
  }
}
