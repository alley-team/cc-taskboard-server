//! Сервер CC TaskBoard.

extern crate base64;
extern crate chrono;
extern crate custom_error;
extern crate passwords;

mod hyper_cli;
mod hyper_router;
mod model;
mod psql_handler;
mod sec;
mod setup;

use psql_handler::Db;

#[tokio::main]
pub async fn main() {
  let cfg = setup::get_config();
  let port = cfg.hyper_port;
  let manager = bb8_postgres::PostgresConnectionManager::new_from_stringlike(
                    cfg.pg.clone(),
                    tokio_postgres::NoTls)
                  .unwrap();
  let pool = bb8::Pool::builder()
    .max_size(15)
    .build(manager)
    .await
    .unwrap();
  let db = Db::new(pool);
  let service = hyper::service::make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
    let local_cfg = cfg.clone();
    let db = db.clone();
    let addr = conn.remote_addr();
    let service = hyper::service::service_fn(move |req| {
      hyper_router::router(local_cfg.clone(),
                           db.clone(),
                           addr,
                           req)
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
  unimplemended!();
}
