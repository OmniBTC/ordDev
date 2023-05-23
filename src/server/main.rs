use anyhow::Error;
use bitcoin::Address;
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, StatusCode};
use ord::options::Options;
use ord::subcommand::wallet::mint_brc20::MintBrc20;
use ord::FeeRate;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintBrc20Param {
  fee_rate: u64,
  source: Address,
  content: String,
  destination: Option<Address>,
  extension: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintBrc20Data {
  jsonrpc: Option<String>,
  id: Option<u32>,
  method: String,
  params: MintBrc20Param,
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Error> {
  match (req.method(), req.uri().path()) {
    (&Method::GET, "/") => {
      // 处理GET请求
      let response_body = "Hello, GET request!";
      Ok(Response::new(Body::from(response_body)))
    }
    (&Method::POST, "/") => {
      // 处理POST请求
      let full_body = hyper::body::to_bytes(req.into_body()).await?;
      let decoded_body = String::from_utf8_lossy(&full_body).to_string();
      println!("{}", decoded_body.clone());
      let form_data: MintBrc20Data = match serde_json::from_str(&decoded_body) {
        Ok(data) => data,
        Err(_) => {
          return Ok(Response::new(Body::from("Invalid form data")));
        }
      };

      match form_data.method.as_str() {
        "mint_brc20" => {
          println!("{:?}", form_data);
          let mint_brc20 = MintBrc20 {
            fee_rate: FeeRate::from(form_data.params.fee_rate),
            destination: form_data.params.destination,
            source: form_data.params.source,
            extension: form_data.params.extension,
            content: form_data.params.content,
          };
          mint_brc20.build(Options {
            bitcoin_data_dir: None,
            bitcoin_rpc_pass: None,
            bitcoin_rpc_user: None,
            chain_argument: Default::default(),
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
          Ok(Response::new(Body::from("Form data received")))
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
  let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
  let make_svc =
    make_service_fn(|_conn| async { Ok::<_, hyper::Error>(service_fn(handle_request)) });

  let server = Server::bind(&addr).serve(make_svc);
  println!("Server running at http://{}", addr);

  if let Err(e) = server.await {
    eprintln!("Server error: {}", e);
  }
}
