use hyper::{Body, Method};

use hyper::service::{make_service_fn, service_fn};
use hyper::server::conn::AddrStream;
use tokio_postgres::{NoTls, Error as PgError, Client as PgClient};
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};
use std::{io, io::{Error as IOErr, ErrorKind as IOErrKind},
          error::Error as StdError, 
          // sync::{Arc, Mutex}, 
          boxed::Box, 
          net::SocketAddr};
mod setup, hyper_router;
extern crate crypto;
extern crate passwords;

#[tokio::main]
pub async fn main() {
  let cfg = setup::get_config();
  
  let service = hyper::service::make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
    let local_cfg = cfg.clone();
    let addr = conn.remote_addr();
    let service = hyper::service::service_fn(move |req| { 
      hyper_router::router(local_cfg.clone(), addr, req)
    });
    async move { Ok::<_, std::convert::Infallible>(service) }
  });
  
  let addr = ([127, 0, 0, 1], cfg.hyper_port).into();
  let server = hyper::Server::bind(&addr).serve(service);
  println!("Сервер слушает по адресу http://{}", addr);
  
  let finisher = server.with_graceful_shutdown(hyper_router::shutdown());
  match finisher.await {
    Err(e) => eprintln!("Ошибка сервера: {}", e),
    _ => println!("\nСервер успешно выключен.")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  
}
