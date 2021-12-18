//! Модуль hyper_router отвечает за управление аутентификацией и вызов необходимых методов работы с базами данных.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use hyper::{Body, Method};
use hyper::body::to_bytes as body_to_bytes;
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};

pub mod data;
pub mod auth;
mod std_routes;

use crate::psql_handler;
use crate::sec::auth::{UserAuth, RegisterUserData};
use crate::sec::tokens_vld;
use crate::setup::AppConfig;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

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
                    body_to_bytes(ws.req.into_body()).await.unwrap()).unwrap() == ws.cnf.admin_key {
      false => StatusCode::UNAUTHORIZED,
      true => match psql_handler::db_setup(ws.cli).await {
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
  Ok(match serde_json::from_str::<RegisterUserData>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => std_routes::route_400(),
    Ok(register_data) => match psql_handler::check_cc_key(Arc::clone(&ws.cli), register_data.cc_key.clone()).await {
      Err(_) => std_routes::route_401(),
      Ok(key_id) => {
        if let Err(res) = psql_handler::remove_cc_key(Arc::clone(&ws.cli), key_id).await {
          return Ok(std_routes::route_401());
        };
        match psql_handler::create_user(Arc::clone(&ws.cli), register_data).await {
          Err(_) => std_routes::route_500(),
          Ok(id) => match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
            Err(_) => std_routes::route_500(),
            Ok(token_auth) => Response::new(Body::from(token_auth)),
          },
        }
      }
    },
  })
}

/// Отвечает за аутентификацию пользователей в приложении.
async fn sign_in_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<UserAuth>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => std_routes::route_400(),
    Ok(user_auth) => match psql_handler::user_credentials_to_id(Arc::clone(&ws.cli), user_auth).await {
      Err(_) => std_routes::route_401(),
      Ok(id) => {
        if id == -1 {
          std_routes::route_401()
        } else { 
          match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
            Err(_) => std_routes::route_500,
            Ok(token_auth) => Response::new(Body::from(token_auth)),
          }
        }
      },
    },
  })
}

// Все следующие методы обязаны содержать в теле запроса JSON с TokenAuth.

/// Создаёт пейдж для пользователя.
async fn create_page_route(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<serde_json::Value>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => std_routes::route_400(),
    Ok(create_page_task) => {
      let token_auth: TokenAuth = serde_json::from_str(create_page_task["token_auth"]);
      match token_auth {
        Err(_) => std_routes::route_400(),
        Ok(token_auth) => match tokens_vld::verify_token(Arc::clone(&ws.cli), token_auth) {
          false => std_routes::route_401(),
          true => match psql_handler::
        }
      }
    },
  })
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
  let (cli, con) = tokio_postgres::connect(cnf.pg_config.as_str(), tokio_postgres::NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = con.await { eprintln!("Ошибка подключения к PostgreSQL: {}", e); }
  });
  let cli = Arc::new(Mutex::new(cli));
  let ws = Workspace { req, cli: Arc::clone(&cli), cnf };
  Ok(match (ws.req.method(), ws.req.uri().path()) {
    (&Method::POST, "/pg-setup") => std_routes::ok_or_401(db_setup_route(ws).await),
    (&Method::POST, "/sign-up") => std_routes::ok_or_400(sign_up_route(ws).await),
    (&Method::POST, "/sign-in") => std_routes::ok_or_401(sign_in_route(ws).await),
    (&Method::POST, "/create-page") => std_routes::ok_or_401(create_page_route(ws).await),
    _ => std_routes::route_404(),
  })
}
