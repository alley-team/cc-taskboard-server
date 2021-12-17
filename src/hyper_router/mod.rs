use tokio_postgres::{NoTls, Client as PgClient};
use std::net::SocketAddr;
use hyper::{Body, Method};
use hyper::body::to_bytes as body_to_bytes;
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};

mod data;
mod postgres_fns;
mod std_routes;
use super::setup::AppConfig;

//! Модуль hyper_router отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

/// Объединяет окружение в одну структуру данных.
struct Workspace {
  req: Request<Body>,
  cli: PgClient,
  cnf: AppConfig,
}

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
async fn db_setup_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match data::parse_admin_auth_key(
                    body_to_bytes(ws.req.into_body()).await).unwrap() == ws.cnf.admin_key {
      false => StatusCode::UNAUTHORIZED,
      true => match postgres_fns::db_setup(ws.cli).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
      }
    })
    .body(Body::empty())?)
}

/// Отвечает за регистрацию нового пользователя. 
/// 
/// Поведение (при условии валидности всех остальных данных):
/// 1. Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор), если отсутствуют данные о токенах.
/// 2. Обновляет данные о токенах, если аккаунт существует, и токены переданы.
/// 3. Создаёт аккаунт и запоминает переданные токены, возвращая только идентификатор, если аккаунта не существует и токены (хотя бы один) переданы.
async fn sign_up_route(ws: Workspace) -> HttpResult<Response<Body>> {
  let builder = Response::builder();
  match serde_json::from_str<UserAuthData>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.to_vec()).unwrap())
  {
    Err(_) => builder.status(StatusCode::BAD_REQUEST).body(Body::empty())?,
    Ok(user_auth) => match postgres_fns::check_cc_key(user_auth.cc_key).await {
      Err(_) => builder.status(StatusCode::UNAUTHORIZED).body(Body::empty())?,
    },
  }
}

/// Отвечает за аутентификацию пользователей в приложении.


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
    (&Method::POST, "/pg-setup") => std_routes::ok_or_401(db_setup_route(ws).await),
    (&Method::POST, "/sign-up") => std_routes::ok_or_400(sign_up_route(ws).await),
    _ => std_routes::route_404(),
  })
}
