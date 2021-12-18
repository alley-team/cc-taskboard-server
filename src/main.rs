extern crate passwords;
extern crate chrono;

mod hyper_cli;
mod hyper_router;
mod psql_handler;
mod sec;
mod setup;

#[tokio::main]
pub async fn main() {
  let cfg = setup::get_config();
  let port = cfg.hyper_port;
  
  let service = hyper::service::make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
    let local_cfg = cfg.clone();
    let addr = conn.remote_addr();
    let service = hyper::service::service_fn(move |req| {
      hyper_router::router(local_cfg.clone(), addr, req)
    });
    async move { Ok::<_, std::convert::Infallible>(service) }
  });
  
  let addr = ([127, 0, 0, 1], port).into();
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
