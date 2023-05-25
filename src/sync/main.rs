use clap::{Arg, Command};
use log::{error, info};
use ord::chain::Chain;
use ord::index::Index;
use ord::options::Options;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn main() {
  std::env::set_var("RUST_LOG", "ord_index=info");
  env_logger::init();
  let args = Command::new("Brc20 Server")
    .arg(
      Arg::new("chain")
        .long("chain")
        .takes_value(true)
        .default_value("test")
        .help("Sets the chain"),
    )
    .arg(
      Arg::new("bitcoin-data-dir")
        .long("bitcoin-data-dir")
        .takes_value(true)
        .help("Load Bitcoin Core data dir from <BITCOIN_DATA_DIR>."),
    )
    .arg(
      Arg::new("bitcoin-rpc-pass")
        .long("bitcoin-rpc-pass")
        .takes_value(true)
        .help("Authenticate to Bitcoin Core RPC with <RPC_PASS>."),
    )
    .arg(
      Arg::new("bitcoin-rpc-user")
        .long("bitcoin-rpc-user")
        .takes_value(true)
        .help("Authenticate to Bitcoin Core RPC as <RPC_USER>."),
    )
    .arg(
      Arg::new("rpc-url")
        .long("rpc-url")
        .takes_value(true)
        .help("Connect to Bitcoin Core RPC at <RPC_URL>."),
    )
    .arg(
      Arg::new("wait-start")
        .long("wait-start")
        .takes_value(true)
        .help("Wait to start up."),
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

  let bitcoin_data_dir: Option<PathBuf> = matches
    .get_one::<String>("bitcoin-data-dir")
    .map(|s| s.into());

  let bitcoin_rpc_pass = matches
    .get_one::<String>("bitcoin-rpc-pass")
    .map(|s| s.clone());

  let bitcoin_rpc_user = matches
    .get_one::<String>("bitcoin-rpc-user")
    .map(|s| s.clone());

  let wait_start = matches
    .get_one::<u64>("wait-start")
    .map(|s| *s);

  if let Some(w) = wait_start{
    info!("Wait {w}s to start...");
    thread::sleep(Duration::from_secs(w));
  }

  let rpc_url = matches.get_one::<String>("rpc-url").map(|s| s.clone());

  let options = Options {
    bitcoin_data_dir,
    bitcoin_rpc_pass,
    bitcoin_rpc_user,
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
    rpc_url,
    signet: false,
    testnet: false,
    wallet: "ord".to_string(),
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
