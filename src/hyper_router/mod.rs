use tokio_postgres::{NoTls, Client as PgClient};
use hyper::{Body, Method};
use std::net::SocketAddr;
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};

mod data;
mod postgres_fns;
use super::setup::AppConfig;

/// Выдаёт информацию об ошибке.
fn route_404() -> Response<Body> {
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(Body::empty()).unwrap()
}

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
async fn db_setup(
    req: Request<Body>,
    cli: PgClient,
    cont: AppConfig,
) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match hyper::body::to_bytes(req.into_body()).await {
      Err(_) => StatusCode::UNAUTHORIZED,
      Ok(bytes) => match data::parse_admin_auth_key(bytes).unwrap() == cont.admin_key {
        false => StatusCode::UNAUTHORIZED,
        true => match postgres_fns::db_setup(cli).await {
          Ok(_) => StatusCode::OK,
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
      }
    })
    .body(Body::empty())?)
}

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
    context: AppConfig,
    _addr: SocketAddr,
    req: Request<Body>
) -> Result<Response<hyper::Body>, std::convert::Infallible> {
  let (pg_client, pg_connection) = tokio_postgres::connect(context.pg_config.as_str(), NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = pg_connection.await {
      eprintln!("Ошибка подключения к PostgreSQL: {}", e);
    }
  });
  Ok(match (req.method(), req.uri().path()) {
    (&Method::POST, "/pg-setup") => ok_or_404(db_setup(req, pg_client, context).await),
    _ => route_404(),
  })
}
