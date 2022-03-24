//! Отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

use hyper::{Body, Method, http::{Request, Response}};
use std::{convert::Infallible, net::SocketAddr};

mod resp;
mod routes;

use crate::model::Workspace;
use crate::psql_handler::Db;

/// Обрабатывает сигнал завершения работы сервера.
pub async fn shutdown() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

/// Обрабатывает запросы клиентов.
pub async fn router(req: Request<Body>, db: Db, admin_key: String, _addr: SocketAddr)
  -> Result<Response<Body>, Infallible>
{
  let ws = Workspace { req, db };
  Ok(match (ws.req.method(), ws.req.uri().path()) {
    (    &Method::GET,     "/pg-setup")     => routes::db_setup           (ws, admin_key)      .await,
    (    &Method::GET,     "/cc-key")       => routes::get_new_cc_key     (ws, admin_key)      .await,
    (    &Method::PUT,     "/sign-up")      => routes::sign_up            (ws)                 .await,
    (    &Method::GET,     "/sign-in")      => routes::sign_in            (ws)                 .await,
    (    &Method::OPTIONS, _)               => routes::pre_request        ()                   .await,
    (method, path) => match routes::auth_by_token(&ws).await {
      Ok((user_id, billed)) => match (method, path) {
        (&Method::PUT,     "/board")        => routes::create_board       (ws, user_id, billed).await,
        (&Method::POST,    "/board")        => routes::get_board          (ws, user_id)        .await,
        (&Method::PATCH,   "/board")        => routes::patch_board        (ws, user_id)        .await,
        (&Method::DELETE,  "/board")        => routes::delete_board       (ws, user_id)        .await,
        (&Method::PUT,     "/card")         => routes::create_card        (ws, user_id)        .await,
        (&Method::PATCH,   "/card")         => routes::patch_card         (ws, user_id)        .await,
        (&Method::DELETE,  "/card")         => routes::delete_card        (ws, user_id)        .await,
        (&Method::PUT,     "/task")         => routes::create_task        (ws, user_id)        .await,
        (&Method::PATCH,   "/task")         => routes::patch_task         (ws, user_id)        .await,
        (&Method::DELETE,  "/task")         => routes::delete_task        (ws, user_id)        .await,
        (&Method::PATCH,   "/task/tags")    => routes::patch_task_tags    (ws, user_id)        .await,
        (&Method::PATCH,   "/task/time")    => routes::patch_task_time    (ws, user_id)        .await,
        (&Method::PUT,     "/subtask")      => routes::create_subtask     (ws, user_id)        .await,
        (&Method::PATCH,   "/subtask")      => routes::patch_subtask      (ws, user_id)        .await,
        (&Method::DELETE,  "/subtask")      => routes::delete_subtask     (ws, user_id)        .await,
        (&Method::PATCH,   "/subtask/tags") => routes::patch_subtask_tags (ws, user_id)        .await,
        (&Method::PATCH,   "/subtask/time") => routes::patch_subtask_time (ws, user_id)        .await,
        (&Method::PATCH,   "/user/creds")   => routes::patch_user_creds   (ws, user_id)        .await,
        (&Method::PATCH,   "/user/billing") => routes::patch_user_billing (ws, user_id)        .await,
        _ => resp::from_code_and_msg(404, Some("Запрашиваемый ресурс не существует.")),
      },
      Err((code, msg)) => resp::from_code_and_msg(code, Some(&msg)),
    },
  })
}
