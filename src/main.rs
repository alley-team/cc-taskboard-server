//! Сервер CC TaskBoard.

mod core;
mod hyper_router;
mod model;
mod psql_handler;
mod sec;
mod setup;

use psql_handler::Db;

#[tokio::main]
pub async fn main() {
  let cfg = setup::get_config();
  let manager = bb8_postgres::PostgresConnectionManager::new_from_stringlike(
                    cfg.pg.clone(),
                    tokio_postgres::NoTls)
                  .unwrap();
  let pool = bb8::Pool::builder().max_size(15).build(manager).await.unwrap();
  let db = Db::new(pool);
  let service = hyper::service::make_service_fn(move |conn: &hyper::server::conn::AddrStream| {
    let db = db.clone();
    let admin_key = cfg.admin_key.clone();
    let addr = conn.remote_addr();
    let service = hyper::service::service_fn(move |req| {
      hyper_router::router(req, db.clone(), admin_key.clone(), addr)
    });
    async move { Ok::<_, std::convert::Infallible>(service) }
  });
  let server = hyper::Server::bind(&cfg.hyper_addr).serve(service);
  println!("Сервер слушает по адресу http://{}", cfg.hyper_addr);
  let finisher = server.with_graceful_shutdown(hyper_router::shutdown());
  match finisher.await {
    Err(e) => eprintln!("Ошибка сервера: {}", e),
    _ => println!("\nСервер успешно выключен."),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  unimplemended!();
}
