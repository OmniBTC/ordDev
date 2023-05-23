use std::net::SocketAddr;
use bitcoin::Address;
use hyper::{Body, Request, Response, Method, StatusCode};
use hyper::server::Server;
use hyper::service::{make_service_fn, service_fn};
use serde::{Deserialize, Serialize};
use ord::subcommand::wallet::mint_brc20::MintBrc20;

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintBrc20Param {
    fee_rate: u64,
    destination: Option<Address>,
    source: Address,
    extension: Option<String>,
    content: String
}

#[derive(Clone, PartialEq, Eq, Debug, Deserialize, Serialize)]
struct MintBrc20Data {
    jsonrpc: Option<String>,
    id: Option<u32>,
    method: String,
    param: MintBrc20Param
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
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

      // MintBrc20{
      //
      // };

      println!("{:?}", form_data);

      Ok(Response::new(Body::from("Form data received")))
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
  let make_svc = make_service_fn(|_conn| {
    async {
      Ok::<_, hyper::Error>(service_fn(handle_request))
    }
  });

  let server = Server::bind(&addr).serve(make_svc);
  println!("Server running at http://{}", addr);

  if let Err(e) = server.await {
    eprintln!("Server error: {}", e);
  }
}
