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
    (&Method::GET,    "/pg-setup")     => routes::db_setup          (ws).await,
    (&Method::GET,    "/cc-key")       => routes::get_new_cc_key    (ws).await,
    (&Method::PUT,    "/sign-up")      => routes::sign_up           (ws).await,
    (&Method::GET,    "/sign-in")      => routes::sign_in           (ws).await,
    (&Method::PUT,    "/board")        => routes::create_board      (ws).await,
    (&Method::GET,    "/board")        => routes::get_board         (ws).await,
    (&Method::PATCH,  "/board")        => routes::patch_board       (ws).await,
    (&Method::DELETE, "/board")        => routes::delete_board      (ws).await,
//     (&Method::PUT,    "/card")         => routes::create_card       (ws).await,
//     (&Method::PATCH,  "/card")         => routes::patch_card        (ws).await,
//     (&Method::DELETE, "/card")         => routes::delete_card       (ws).await,
//     (&Method::PUT,    "/task")         => routes::create_task       (ws).await,
//     (&Method::PATCH,  "/task")         => routes::patch_task        (ws).await,
//     (&Method::DELETE, "/task")         => routes::delete_task       (ws).await,
//     (&Method::PATCH,  "/task/tags")    => routes::patch_task_tags   (ws).await,
//     (&Method::PATCH,  "/task/time")    => routes::patch_task_time   (ws).await,
//     (&Method::PUT,    "/subtask")      => routes::create_subtask    (ws).await,
//     (&Method::PATCH,  "/subtask")      => routes::patch_subtask     (ws).await,
//     (&Method::DELETE, "/subtask")      => routes::delete_subtask    (ws).await,
//     (&Method::PATCH,  "/subtask/tags") => routes::patch_subtask_tags(ws).await,
//     (&Method::PATCH,  "/subtask/time") => routes::patch_subtask_time(ws).await,
//     (&Method::PATCH,  "/user/creds")   => routes::patch_user_creds  (ws).await,
//     (&Method::PATCH,  "/user/billing") => routes::patch_user_billing(ws).await,
    _ => resp::from_code_and_msg(404, Some(String::from("Запрашиваемый ресурс не существует."))),
  })
}
