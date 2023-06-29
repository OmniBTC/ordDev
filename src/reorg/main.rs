use bitcoin::Network;
use clap::{Arg, Command};
use log::{error, info};
use ord::chain::Chain;
use ord::index::{Index, MysqlDatabase};
use ord::options::Options;
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
  std::env::set_var("RUST_LOG", "info");
  env_logger::init();
  let args = Command::new("Reorg")
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
      Arg::new("data-dir")
        .long("data-dir")
        .takes_value(true)
        .help("Store index in <DATA_DIR>."),
    )
    .arg(
      Arg::new("rpc-url")
        .long("rpc-url")
        .takes_value(true)
        .help("Connect to Bitcoin Core RPC at <RPC_URL>."),
    )
    .arg(
      Arg::new("mysql-host")
        .long("mysql-host")
        .takes_value(true)
        .help("Mysql host."),
    )
    .arg(
      Arg::new("mysql-username")
        .long("mysql-username")
        .takes_value(true)
        .help("Mysql username."),
    )
    .arg(
      Arg::new("mysql-password")
        .long("mysql-password")
        .takes_value(true)
        .help("Mysql password."),
    )
    .arg(
      Arg::new("target-height")
        .long("target-height")
        .takes_value(true)
        .help("Target height."),
    );

  let matches = args.get_matches();
  let chain = matches
    .get_one::<String>("chain")
    .map(|s| s.as_str())
    .unwrap();

  let chain_argument = match chain {
    "main" => Chain::Mainnet,
    "regtest" => Chain::Regtest,
    "signet" => Chain::Signet,
    _ => Chain::Testnet,
  };

  let network = match chain {
    "main" => Network::Bitcoin,
    "regtest" => Network::Regtest,
    "signet" => Network::Signet,
    _ => Network::Testnet,
  };

  let bitcoin_data_dir: Option<PathBuf> = matches
    .get_one::<String>("bitcoin-data-dir")
    .map(|s| s.into());

  let bitcoin_rpc_pass = matches.get_one::<String>("bitcoin-rpc-pass").cloned();

  let bitcoin_rpc_user = matches.get_one::<String>("bitcoin-rpc-user").cloned();

  let data_dir: Option<PathBuf> = matches.get_one::<String>("data-dir").map(|s| s.into());

  let mysql_host = matches.get_one::<String>("mysql-host").cloned();
  let mysql_username = matches.get_one::<String>("mysql-username").cloned();
  let mysql_password = matches.get_one::<String>("mysql-password").cloned();

  let rpc_url = matches.get_one::<String>("rpc-url").cloned();

  let target_height: u64 = matches
    .get_one::<String>("target-height")
    .map(|s| s.parse().expect("Target height must right"))
    .unwrap();

  let options = Options {
    bitcoin_data_dir,
    bitcoin_rpc_pass,
    bitcoin_rpc_user,
    chain_argument,
    config: None,
    config_dir: None,
    cookie_file: None,
    data_dir,
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

  let database = if mysql_host.is_none() || mysql_username.is_none() || mysql_password.is_none() {
    info!("Use redb...");
    None
  } else {
    info!("Use mysql...");
    Some(Arc::new(
      MysqlDatabase::new(mysql_host, mysql_username, mysql_password, network).unwrap(),
    ))
  };

  let open_result = if let Some(db) = database {
    Index::open_with_mysql(&options, db)
  } else {
    Index::open(&options)
  };

  match open_result {
    Ok(index) => {
      if let Err(e) = index.reorg_height(target_height) {
        error!("Index reorg error:{e}")
      } else {
        info!("Index reorg success")
      }
    }
    Err(e) => {
      error!("Index reorg error:{e}")
    }
  }
}
