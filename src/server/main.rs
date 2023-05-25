use anyhow::Error;
use bitcoin::Address;
use clap::{Arg, Command};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode};
use ord::chain::Chain;
use ord::options::Options;
use ord::outgoing::Outgoing;
use ord::subcommand::wallet::mint::Mint;
use ord::subcommand::wallet::transfer::Transfer;
use ord::FeeRate;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
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

async fn handle_request(
  chain_argument: Chain,
  service_address: Address,
  req: Request<Body>,
) -> Result<Response<Body>, Error> {
  let path: Vec<&str> = req.uri().path().split('/').skip(1).collect();
  match (req.method(), path.get(0)) {
    (&Method::GET, Some(&"/")) => {
      // 处理GET请求
      let response_body = "Hello, GET request!";
      Ok(Response::new(Body::from(response_body)))
    }
    (&Method::POST, Some(&"mint")) => {
      // 处理POST请求
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();
      println!("{}", decoded_body.clone());

      let form_data: MintData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };

      match form_data.method.as_str() {
        "mint" => {
          let mint = Mint {
            fee_rate: FeeRate::from(form_data.params.fee_rate),
            destination: form_data.params.destination,
            source: form_data.params.source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
          };
          let output = mint.build(
            Options {
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
            },
            Some(service_address),
          )?;
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
      println!("{}", decoded_body.clone());

      let form_data: TransferData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };

      match form_data.method.as_str() {
        "transfer" => {
          let transfer = Transfer {
            fee_rate: FeeRate::from(form_data.params.fee_rate),
            destination: form_data.params.destination,
            source: form_data.params.source,
            outgoing: Outgoing::from_str(&form_data.params.outgoing)?,
          };
          let output = transfer.build(Options {
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
          })?;
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

#[tokio::main]
async fn main() {
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
    );

  let matches = args.get_matches();
  let chain = matches
    .get_one::<String>("chain")
    .map(|s| s.as_str())
    .unwrap();
  let service_address: Address = Address::from_str(
    &matches
      .get_one::<String>("service_address")
      .map(|s| s.as_str())
      .unwrap(),
  )
  .unwrap();

  let chain_argument = match chain {
    "main" => Chain::Mainnet,
    _ => Chain::Testnet,
  };

  let addr = SocketAddr::from(([127, 0, 0, 1], 3080));
  println!(
    "Server running at http://{}, network:{:?}, service:{:?}",
    addr,
    chain_argument,
    service_address.clone()
  );
  let make_svc = make_service_fn(move |_conn| {
    let chain_argument = chain_argument.clone();
    let service_address = service_address.clone();
    async move {
      Ok::<_, Error>(service_fn(move |req| {
        handle_request(chain_argument.clone(), service_address.clone(), req)
      }))
    }
  });

  let server = Server::bind(&addr).serve(make_svc);

  if let Err(e) = server.await {
    eprintln!("Server error: {}", e);
  }
}
