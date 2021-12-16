use tokio_postgres::{NoTls, Client as PgClient};
use hyper::{Body, Method};
use std::net::SocketAddr;
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};

mod data;
mod postgres_fns;
use super::setup::AppConfig;

//! Модуль hyper_router отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

/// Объединяет окружение в одну структуру данных.
struct Workspace {
  req: Request<Body>,
  cli: PgClient,
  cnf: AppConfig,
}

/// Выдаёт информацию об ошибке.
fn route_404() -> Response<Body> {
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(Body::empty()).unwrap()
}

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
async fn db_setup_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match hyper::body::to_bytes(ws.req.into_body()).await {
      Err(_) => StatusCode::UNAUTHORIZED,
      Ok(bytes) => match data::parse_admin_auth_key(bytes).unwrap() == ws.cnf.admin_key {
        false => StatusCode::UNAUTHORIZED,
        true => match postgres_fns::db_setup(ws.cli).await {
          Ok(_) => StatusCode::OK,
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
      }
    })
    .body(Body::empty())?)
}

/// Отвечает за регистрацию нового пользователя.
async fn sign_up_route(ws: Workspace) -> HttpResult<Response<Body>> {
  
}

/// Отвечает за аутентификацию пользователей в приложении.


/// Если функция вернула Result::Ok(resp), возвращает resp; иначе - 404.
fn ok_or_404(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_404(),
  }
}

// Публичные функции:

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
) -> Result<Response<hyper::Body>, std::convert::Infallible> {
  let (cli, con) = tokio_postgres::connect(cnf.pg_config.as_str(), NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = con.await { eprintln!("Ошибка подключения к PostgreSQL: {}", e); }
  });
  let ws = Workspace { req, cli, cnf };
  Ok(match (req.method(), req.uri().path()) {
    (&Method::POST, "/pg-setup") => ok_or_404(db_setup_route(ws).await),
    (&Method::POST, "/auth") => ok_or_404(sign_up_route(ws).await),
    _ => route_404(),
  })
}
