use std::thread;
use std::time::Duration;
use clap::{Arg, Command};
use env_logger;
use log::{error, info};
use ord::chain::Chain;
use ord::index::Index;
use ord::options::Options;

fn main() {
  std::env::set_var("RUST_LOG", "ord_index=info");
  env_logger::init();
  let args = Command::new("Brc20 Server").arg(
    Arg::new("chain")
      .long("chain")
      .takes_value(true)
      .default_value("test")
      .help("Sets the chain"),
  );

  let matches = args.get_matches();
  let chain = matches
    .get_one::<String>("chain")
    .map(|s| s.as_str())
    .unwrap();

  let chain_argument = match chain {
    "main" => Chain::Mainnet,
    _ => Chain::Testnet,
  };

  let options = Options {
    bitcoin_data_dir: None,
    bitcoin_rpc_pass: None,
    bitcoin_rpc_user: None,
    chain_argument,
    config: None,
    config_dir: None,
    cookie_file: None,
    data_dir: None,
    first_inscription_height: None,
    height_limit: None,
    index: None,
    index_sats: false,
    regtest: false,
    rpc_url: None,
    signet: false,
    testnet: false,
    wallet: "".to_string(),
  };
  let mut count = 0;
  loop {
    if count > 0 {
      thread::sleep(Duration::from_secs(180));
    }
    let index = Index::open(&options).unwrap();
    if let Err(e) = index.update() {
      error!("Index error:{e}")
    } else {
      info!("Index success")
    }
    count += 1;
  }
}
