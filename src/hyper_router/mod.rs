//! Модуль hyper_router отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

use tokio_postgres::{NoTls, Client as PgClient};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use hyper::{Body, Method};
use hyper::body::to_bytes as body_to_bytes;
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};

mod data;
mod postgres_fns;
mod std_routes;
use super::setup::AppConfig;
use data::UserAuthData;

/// Объединяет окружение в одну структуру данных.
struct Workspace {
  req: Request<Body>,
  cli: Arc<Mutex<PgClient>>,
  cnf: AppConfig,
}

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
async fn db_setup_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match data::parse_admin_auth_key(
                    body_to_bytes(ws.req.into_body()).await.unwrap()).unwrap() == ws.cnf.admin_key {
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
/// Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор).
async fn sign_up_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<UserAuthData>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => std_routes::route_400(),
    Ok(user_auth) => match postgres_fns::check_cc_key(Arc::clone(&ws.cli), user_auth.cc_key.clone()).await {
      Err(_) => std_routes::route_401(),
      Ok(key_id) => {
        if let Err(res) = postgres_fns::remove_cc_key(Arc::clone(&ws.cli), key_id).await {
          return Ok(std_routes::route_401());
        };
        match postgres_fns::create_user(Arc::clone(&ws.cli), user_auth).await {
          Err(_) => std_routes::route_500(),
          Ok(id) => match postgres_fns::get_new_token(Arc::clone(&ws.cli), id).await {
            Err(_) => std_routes::route_500(),
            Ok(token) => Response::new(Body::from(token)),
          },
        }
      }
    },
  })
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
  let cli = Arc::new(Mutex::new(cli));
  let ws = Workspace { req, cli: Arc::clone(&cli), cnf };
  Ok(match (ws.req.method(), ws.req.uri().path()) {
    (&Method::POST, "/pg-setup") => std_routes::ok_or_401(db_setup_route(ws).await),
    (&Method::POST, "/sign-up") => std_routes::ok_or_400(sign_up_route(ws).await),
    _ => std_routes::route_404(),
  })
}
