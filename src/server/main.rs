use anyhow::Error;
use bitcoin::Address;
use clap::{Arg, Command};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode};
use log::{error, info};
use ord::chain::Chain;
use ord::options::Options;
use ord::outgoing::Outgoing;
use ord::subcommand::wallet::mint::Mint;
use ord::subcommand::wallet::transfer::Transfer;
use ord::FeeRate;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintParam {
  fee_rate: u64,
  source: Address,
  content: String,
  destination: Option<Address>,
  extension: Option<String>,
  repeat: Option<u64>,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintParam,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct TransferParam {
  source: Address,
  destination: Address,
  outgoing: String,
  fee_rate: u64,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct TransferData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: TransferParam,
}

async fn _handle_request(
  options: Options,
  service_address: Address,
  req: Request<Body>,
) -> Result<Response<Body>, Error> {
  let path: Vec<&str> = req.uri().path().split('/').skip(1).collect();
  match (req.method(), path.first()) {
    (&Method::GET, Some(&"/")) => {
      // 处理GET请求
      let response_body = "Hello, GET request!";
      Ok(Response::new(Body::from(response_body)))
    }
    (&Method::POST, Some(&"mint")) => {
      // 处理POST请求
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: MintData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };
      let source = form_data.params.source;
      let destination = form_data
        .params
        .destination
        .clone()
        .unwrap_or(source.clone());
      info!("Mint from {source} to {destination}");

      match form_data.method.as_str() {
        "mint" => {
          let mint = Mint {
            fee_rate: FeeRate::from(form_data.params.fee_rate),
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
          };
          let output = mint.build(options, Some(service_address))?;
          Ok(Response::new(Body::from(serde_json::to_string(&output)?)))
        }
        _ => {
          let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Method not found"))
            .unwrap();
          Ok(response)
        }
      }
    }
    (&Method::POST, Some(&"transfer")) => {
      // 处理POST请求
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: TransferData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };
      let source = form_data.params.source;
      let destination = form_data.params.destination;
      info!("Transfer from {source} to {destination}");

      match form_data.method.as_str() {
        "transfer" => {
          let transfer = Transfer {
            fee_rate: FeeRate::from(form_data.params.fee_rate),
            destination,
            source,
            outgoing: Outgoing::from_str(&form_data.params.outgoing)?,
          };
          let output = transfer.build(options)?;
          Ok(Response::new(Body::from(serde_json::to_string(&output)?)))
        }
        _ => {
          let response = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Method not found"))
            .unwrap();
          Ok(response)
        }
      }
    }
    _ => {
      // 处理其他请求
      let response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap();
      Ok(response)
    }
  }
}

async fn handle_request(
  options: Options,
  service_address: Address,
  req: Request<Body>,
) -> Result<Response<Body>, Error> {
  match _handle_request(options, service_address, req).await {
    Ok(v) => Ok(v),
    Err(e) => {
      error!("Req fail:{e}");
      Ok(
        Response::builder()
          .status(StatusCode::BAD_REQUEST)
          .body(Body::from(format!("{}", e)))
          .unwrap(),
      )
    }
  }
}

#[tokio::main]
async fn main() {
  std::env::set_var("RUST_LOG", "ord_server=info");
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
      Arg::new("service_address")
        .long("service_address")
        .takes_value(true)
        .help("Sets the service address"),
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
      Arg::new("ip")
        .long("ip")
        .takes_value(true)
        .default_value("0.0.0.0")
        .help("Connect to Bitcoin Core RPC at <RPC_URL>."),
    );

  let matches = args.get_matches();
  let chain = matches
    .get_one::<String>("chain")
    .map(|s| s.as_str())
    .unwrap();
  let service_address: Address = Address::from_str(
    matches
      .get_one::<String>("service_address")
      .map(|s| s.as_str())
      .unwrap(),
  )
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

  let data_dir: Option<PathBuf> = matches.get_one::<String>("data-dir").map(|s| s.into());

  let rpc_url = matches.get_one::<String>("rpc-url").map(|s| s.clone());

  let ip = matches.get_one::<String>("ip").map(|s| s.clone()).unwrap();

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

  let addr = SocketAddr::new(ip.as_str().parse().unwrap(), 3080);
  info!(
    "Server running at http://{}, network:{:?}, service:{:?}",
    addr,
    chain_argument,
    service_address.clone()
  );
  let make_svc = make_service_fn(move |_conn| {
    let options = options.clone();
    let service_address = service_address.clone();
    async move {
      Ok::<_, Error>(service_fn(move |req| {
        handle_request(options.clone(), service_address.clone(), req)
      }))
    }
  });

  let server = Server::bind(&addr).serve(make_svc);

  if let Err(e) = server.await {
    error!("Server error: {}", e);
  }
}
