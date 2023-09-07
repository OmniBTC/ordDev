use anyhow::{anyhow, Error};
use bitcoin::{Address, Amount, Network, OutPoint, Txid};
use clap::{Arg, Command};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode};
use log::{error, info};
use ord::chain::Chain;
use ord::index::MysqlDatabase;
use ord::options::Options;
use ord::outgoing::Outgoing;
use ord::subcommand::wallet::cancel::Cancel;
use ord::subcommand::wallet::mint::Mint;
use ord::subcommand::wallet::mints;
use ord::subcommand::wallet::transfer::Transfer;
use ord::{FeeRate, TransactionBuilder};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tokio::task;

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintParam {
  fee_rate: f64,
  source: Address,
  content: String,
  destination: Option<Address>,
  extension: Option<String>,
  repeat: Option<u64>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct TransferParam {
  source: Address,
  destination: Address,
  outgoing: String,
  fee_rate: f64,
  op_return: String,
  brc20_transfer: bool,
  addition_outgoing: Vec<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct TransferData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: TransferParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct TransferWithFeeParam {
  source: Address,
  destination: Address,
  outgoing: String,
  fee_rate: f64,
  op_return: String,
  brc20_transfer: bool,
  addition_outgoing: Vec<String>,
  addition_fee: u64,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct TransferWithFeeData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: TransferWithFeeParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintsParam {
  fee_rate: f64,
  source: Address,
  content: Vec<String>,
  destination: Option<Address>,
  extension: Option<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintsData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintsParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct CancelParam {
  fee_rate: f64,
  source: Address,
  inputs: Vec<String>,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct CancelData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: CancelParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintWithPostageParam {
  fee_rate: f64,
  source: Address,
  content: String,
  destination: Option<Address>,
  extension: Option<String>,
  repeat: Option<u64>,
  target_postage: u64,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintWithPostageData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintWithPostageParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintsWithPostageParam {
  fee_rate: f64,
  source: Address,
  content: Vec<String>,
  destination: Option<Address>,
  extension: Option<String>,
  target_postage: u64,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct MintsWithPostageData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintsWithPostageParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct ReMintParam {
  fee_rate: f64,
  source: Address,
  content: String,
  destination: Option<Address>,
  extension: Option<String>,
  repeat: Option<u64>,
  target_postage: u64,
  remint: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct ReMintData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: ReMintParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct ReMintsParam {
  fee_rate: f64,
  source: Address,
  content: Vec<String>,
  destination: Option<Address>,
  extension: Option<String>,
  target_postage: u64,
  remint: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct ReMintsData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: ReMintsParam,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct IsWhitelistParam {
  source: String,
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
struct IsWhitelistData {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: IsWhitelistParam,
}

fn add_fee(service_fee: Option<Amount>, add: u64) -> Option<Amount> {
  if let Some(fee) = service_fee {
    Some(fee + Amount::from_sat(add))
  } else {
    Some(Amount::from_sat(add))
  }
}

async fn _handle_request(
  options: Options,
  service_address: Address,
  service_fee: u64,
  mysql: Option<Arc<MysqlDatabase>>,
  req: Request<Body>,
) -> Result<Response<Body>, Error> {
  let path: Vec<&str> = req.uri().path().split('/').skip(1).collect();

  let service_fee = Some(Amount::from_sat(service_fee));
  match (req.method(), path.first()) {
    (&Method::GET, Some(&"query")) => match path.get(1) {
      Some(&"inscription") => {
        let addr = path.get(2).ok_or(anyhow!("not found address"))?;
        let data = mysql
          .ok_or(anyhow!("not database"))?
          .get_inscription_by_address(&(*addr).to_owned())?;
        let json_str = serde_json::to_string(&data).map_err(|_| anyhow!("serde fail"))?;
        Ok(Response::new(Body::from(json_str)))
      }
      _ => Ok(Response::new(Body::from("get not recognize"))),
    },
    (&Method::POST, Some(&"isWhitelist")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: IsWhitelistData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };
      let source = form_data.params.source.clone();
      info!("isWhitelist from {source}");

      match form_data.method.as_str() {
        "isWhitelist" => {
          let data = mysql
            .ok_or(anyhow!("not database"))?
            .is_whitelist(&form_data.params.source);

          let mut output = BTreeMap::new();
          output.insert("is_whitelist", data);
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
    (&Method::POST, Some(&"mint")) => {
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
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
            target_postage: TransactionBuilder::TARGET_POSTAGE,
            remint: None,
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql, false)?;
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
    (&Method::POST, Some(&"mints")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: MintsData = match serde_json::from_str(&decoded_body) {
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
      info!("Mints from {source} to {destination}");

      match form_data.method.as_str() {
        "mints" => {
          let mint = mints::Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            target_postage: TransactionBuilder::TARGET_POSTAGE,
            remint: None,
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql)?;
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
          let op_return = if form_data.params.op_return.is_empty() {
            None
          } else {
            Some(form_data.params.op_return)
          };

          let mut addition_outgoing = vec![];
          for item in form_data.params.addition_outgoing.iter() {
            addition_outgoing.push(Outgoing::from_str(item)?)
          }
          let addition_fee = Amount::from_sat(0);
          let transfer = Transfer {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination,
            source,
            outgoing: Outgoing::from_str(&form_data.params.outgoing)?,
            op_return,
            brc20_transfer: Some(form_data.params.brc20_transfer),
            addition_outgoing,
            addition_fee,
          };
          let output = transfer.build(options, mysql)?;
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
    (&Method::POST, Some(&"transferWithFee")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: TransferWithFeeData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };
      let source = form_data.params.source;
      let destination = form_data.params.destination;
      info!("TransferWithFee from {source} to {destination}");

      match form_data.method.as_str() {
        "transferWithFee" => {
          let op_return = if form_data.params.op_return.is_empty() {
            None
          } else {
            Some(form_data.params.op_return)
          };

          let mut addition_outgoing = vec![];
          for item in form_data.params.addition_outgoing.iter() {
            addition_outgoing.push(Outgoing::from_str(item)?)
          }
          let addition_fee = Amount::from_sat(form_data.params.addition_fee);
          let transfer = Transfer {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination,
            source,
            outgoing: Outgoing::from_str(&form_data.params.outgoing)?,
            op_return,
            brc20_transfer: Some(form_data.params.brc20_transfer),
            addition_outgoing,
            addition_fee,
          };
          let output = transfer.build(options, mysql)?;
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
    (&Method::POST, Some(&"cancel")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: CancelData = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };
      let source = form_data.params.source;
      info!("Cancel from {source}");

      let mut inputs: Vec<OutPoint> = vec![];
      for item in &form_data.params.inputs {
        inputs.push(OutPoint::from_str(item)?);
      }

      match form_data.method.as_str() {
        "cancel" => {
          let cancel = Cancel {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            source,
            inputs,
          };
          let output = cancel.build(
            options,
            Some(service_address),
            Some(Amount::from_sat(0)),
            mysql,
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
    (&Method::POST, Some(&"mintWithPostage")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: MintWithPostageData = match serde_json::from_str(&decoded_body) {
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
      info!("MintWithPostage from {source} to {destination}");

      match form_data.method.as_str() {
        "mintWithPostage" => {
          let mint = Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
            target_postage: Amount::from_sat(form_data.params.target_postage),
            remint: None,
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql, false)?;
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
    (&Method::POST, Some(&"unsafeMintWithPostage")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: MintWithPostageData = match serde_json::from_str(&decoded_body) {
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
      info!("UnsafeMintWithPostage from {source} to {destination}");

      match form_data.method.as_str() {
        "unsafeMintWithPostage" => {
          let mint = Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
            target_postage: Amount::from_sat(form_data.params.target_postage),
            remint: None,
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql, true)?;
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
    (&Method::POST, Some(&"mintsWithPostage")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: MintsWithPostageData = match serde_json::from_str(&decoded_body) {
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
      info!("MintsWithPostage from {source} to {destination}");

      match form_data.method.as_str() {
        "mintsWithPostage" => {
          let mint = mints::Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            target_postage: Amount::from_sat(form_data.params.target_postage),
            remint: None,
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql)?;
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
    (&Method::POST, Some(&"reMint")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: ReMintData = match serde_json::from_str(&decoded_body) {
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
      info!("reMint from {source} to {destination}");

      match form_data.method.as_str() {
        "reMint" => {
          let mint = Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            repeat: form_data.params.repeat,
            target_postage: Amount::from_sat(form_data.params.target_postage),
            remint: Some(Txid::from_str(&form_data.params.remint)?),
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql, true)?;
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
    (&Method::POST, Some(&"reMints")) => {
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();

      let form_data: ReMintsData = match serde_json::from_str(&decoded_body) {
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
      info!("reMints from {source} to {destination}");

      match form_data.method.as_str() {
        "reMints" => {
          let mint = mints::Mint {
            fee_rate: FeeRate::try_from(form_data.params.fee_rate)?,
            destination: form_data.params.destination,
            source,
            extension: form_data.params.extension,
            content: form_data.params.content,
            target_postage: Amount::from_sat(form_data.params.target_postage),
            remint: Some(Txid::from_str(&form_data.params.remint)?),
          };

          let output = mint.build(options, Some(service_address), service_fee, mysql)?;
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
  service_fee: u64,
  mysql: Option<Arc<MysqlDatabase>>,
  req: Request<Body>,
) -> Result<Response<Body>, Error> {
  let result = task::spawn(async move {
    match _handle_request(options, service_address, service_fee, mysql, req).await {
      Ok(v) => Ok(v),
      Err(e) => {
        error!("Req fail:{e}");
        let format_error = format!("{}", e).to_lowercase();
        let final_error = if format_error.contains("database") {
          String::from("API requests are too frequent, please try again later")
        } else {
          format!("{}", e)
        };
        Ok(
          Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from(final_error))
            .unwrap(),
        )
      }
    }
  })
  .await;
  match result {
    Ok(response) => response,
    Err(panic) => {
      error!("Req panic:{panic}");
      Ok(
        Response::builder()
          .status(StatusCode::BAD_REQUEST)
          .body(Body::from(
            "API requests are too frequent, please try again later",
          ))
          .unwrap(),
      )
    }
  }
}

#[tokio::main]
async fn main() {
  std::env::set_var("RUST_LOG", "info");
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
      Arg::new("service-address")
        .long("service-address")
        .takes_value(true)
        .help("Sets the service address"),
    )
    .arg(
      Arg::new("service-fee")
        .long("service-fee")
        .takes_value(true)
        .default_value("3000")
        .help("Sets the service fee"),
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
    );

  let matches = args.get_matches();
  let chain = matches
    .get_one::<String>("chain")
    .map(|s| s.as_str())
    .unwrap();
  let service_address: Address = Address::from_str(
    matches
      .get_one::<String>("service-address")
      .map(|s| s.as_str())
      .unwrap(),
  )
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

  let rpc_url = matches.get_one::<String>("rpc-url").cloned();

  let ip = matches.get_one::<String>("ip").cloned().unwrap();

  let service_fee: u64 = matches
    .get_one::<String>("service-fee")
    .map(|s| s.parse().unwrap_or(3000))
    .unwrap();

  let mysql_host = matches.get_one::<String>("mysql-host").cloned();
  let mysql_username = matches.get_one::<String>("mysql-username").cloned();
  let mysql_password = matches.get_one::<String>("mysql-password").cloned();
  let database = if mysql_host.is_none() || mysql_username.is_none() || mysql_password.is_none() {
    info!("Use redb...");
    None
  } else {
    info!("Use mysql...");
    Some(Arc::new(
      MysqlDatabase::new(mysql_host, mysql_username, mysql_password, network).unwrap(),
    ))
  };

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

  let addr = SocketAddr::new(ip.as_str().parse().unwrap(), 3100);
  info!(
    "Server running at http://{}, network:{:?}, service:{:?}",
    addr,
    chain_argument,
    service_address.clone()
  );
  let make_svc = make_service_fn(move |_conn| {
    let options = options.clone();
    let service_address = service_address.clone();
    let database = database.clone();
    async move {
      Ok::<_, Error>(service_fn(move |req| {
        handle_request(
          options.clone(),
          service_address.clone(),
          service_fee,
          database.clone(),
          req,
        )
      }))
    }
  });

  let server = Server::bind(&addr).serve(make_svc);

  if let Err(e) = server.await {
    error!("Server error: {}", e);
  }
}
