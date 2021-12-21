//! Модуль hyper_router отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use hyper::{Body, Method};
use hyper::http::{Request, Response};

mod resp;
mod routes;

use crate::model::Workspace;
use crate::setup::AppConfig;

/// Обрабатывает сигнал завершения работы сервера.
pub async fn shutdown() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

/// Обрабатывает запросы клиентов.
pub async fn router(
    cnf: AppConfig,
    _addr: SocketAddr,
    req: Request<Body>
) -> Result<Response<Body>, std::convert::Infallible> {
  let (cli, con) = tokio_postgres::connect(cnf.pg_config.as_str(), tokio_postgres::NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = con.await { eprintln!("Ошибка подключения к PostgreSQL: {}", e); }
  });
  let cli = Arc::new(Mutex::new(cli));
  let ws = Workspace { req, cli: Arc::clone(&cli), cnf };
  Ok(match (ws.req.method(), ws.req.uri().path()) {
    (&Method::GET,    "/pg-setup")    => routes::db_setup(ws)    .await,
    (&Method::PUT,    "/sign-up")     => routes::sign_up(ws)     .await,
    (&Method::GET,    "/sign-in")     => routes::sign_in(ws)     .await,
    (&Method::PUT,    "/board")       => routes::create_board(ws).await,
    // TODO
//     (&Method::PATCH,  "/board")       => ,
//     (&Method::DELETE, "/board")       => ,
//     (&Method::PUT,    "/card")        => ,
//     (&Method::PATCH,  "/card")        => ,
//     (&Method::DELETE, "/card")        => ,
    _ => resp::from_code_and_msg(404, Some(String::from("Запрашиваемый ресурс не существует."))),
  })
}
