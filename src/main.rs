use std::convert::Infallible;
use hyper::{Body, Server, Method};
use hyper::body::{to_bytes as hyper_to_bytes, Bytes as HyperBytes};
use hyper::service::{make_service_fn, service_fn};
use hyper::server::conn::AddrStream;
use tokio_postgres::{NoTls, Error as PgError, Client as PgClient, connect as pg_con};
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};
use std::{io, io::{Error as IOErr, ErrorKind as IOErrKind},
          error::Error as StdError, 
          // sync::{Arc, Mutex}, 
          boxed::Box, 
          net::SocketAddr};
use serde_json::{Result as JsonResult, from_str as json_de};
use crate::data::{CCTaskboardAppContext, AdminAuth};

extern crate crypto;
extern crate passwords;

mod data;

fn json_parse_admin_auth_key(bytes: HyperBytes) -> JsonResult<String> {
  let auth: AdminAuth = json_de(&String::from_utf8(bytes.to_vec()).unwrap())?;
  Ok(auth.key)
}

// fn app_get_all(s: String) -> String {
//   String::from("all")
// }

async fn pg_setup(mut cli: PgClient) -> Result<(), PgError> {
  cli.transaction().await?;
  let queries = vec![
    String::from("create table users (id bigserial, shared_pages varchar, auth_data varchar);"),
    String::from("create table pages (id bigserial, title varchar[64], boards varchar, background_color char[7]);"),
    String::from("create table boards (id bigserial, title varchar[64], tasks varchar, color char[7], background_color char[7]);"),
    ];
  for x in &queries {
    cli.query(x, &[]).await?;
  }
  Ok(())
}

fn hyper_404_route() -> Response<Body> {
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(Body::empty()).unwrap()
}

async fn hyper_route_postgres_setup(
    req: Request<Body>,
    cli: PgClient,
    cont: CCTaskboardAppContext,
) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match hyper_to_bytes(req.into_body()).await {
      Err(_) => StatusCode::UNAUTHORIZED,
      Ok(bytes) => match json_parse_admin_auth_key(bytes).unwrap() == cont.admin_key {
        false => StatusCode::UNAUTHORIZED,
        true => match pg_setup(cli).await {
          Ok(_) => StatusCode::OK,
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
      }
    })
    .body(Body::empty())?)
}

/// Осуществляет переадресацию на сервере.
///
/// Обрабатывает каждое соединение с сервером.
async fn hyper_routing(
    context: CCTaskboardAppContext,
    _addr: SocketAddr,
    req: Request<Body>
) -> Result<Response<Body>, Infallible> {
  let (pg_client, pg_connection) = pg_con(context.pg_config.as_str(), NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = pg_connection.await {
      eprintln!("Ошибка подключения к PostgreSQL: {}", e);
    }
  });
  Ok(match (req.method(), req.uri().path()) {
    (&Method::POST, "/pg-setup") => match hyper_route_postgres_setup(req, pg_client, context).await {
      Ok(resp) => resp,
      _ => hyper_404_route(),
    },
    _ => hyper_404_route(),
  })
}

async fn hyper_shutdown_signal() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

/// 
fn setup() -> Result<(String, u16, String), Box<dyn StdError>> {
  let stdin = io::stdin();
  
  println!("Привет! Это сервер CC TaskBoard. Прежде чем мы начнём, заполните несколько параметров.");
  println!("Введите имя пользователя PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.strip_suffix("\n").ok_or("")?;
  let pg_config = String::from("host=localhost user='") + &buffer + &String::from("' password='");
  
  println!("Введите пароль PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.strip_suffix("\n").ok_or("")?;
  let pg_config = pg_config + &buffer + &String::from("'");
  
  println!("Введите номер порта сервера:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.strip_suffix("\n").ok_or("")?;
  let port: u16 = buffer.parse()?;
  
  println!("Введите ключ для аутентификации администратора (минимум 64 символа):");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let admin_auth_key = String::from(buffer.strip_suffix("\n").ok_or("")?);
  
  match admin_auth_key.len() < 64 {
    true => Err(Box::new(IOErr::new(IOErrKind::Other, "Длина ключа администратора меньше 64 символов."))),
    false => Ok((pg_config, port, admin_auth_key)),
  }
}

#[tokio::main]
pub async fn main() {
  let (pg_config, hyper_port, admin_key) = setup().expect("Настройка не завершена.");
  
  let hyper_context = CCTaskboardAppContext { pg_config, admin_key };
  
  let hyper_service = make_service_fn(move |conn: &AddrStream| {
    let context = hyper_context.clone();
    let addr = conn.remote_addr();
    let service = service_fn(move |req| { 
      hyper_routing(context.clone(), addr, req)
    });
    async move { Ok::<_, Infallible>(service) }
  });
  
  let hyper_server_addr = ([127, 0, 0, 1], hyper_port).into();
  
  let hyper_server = Server::bind(&hyper_server_addr).serve(hyper_service);
  println!("Сервер слушает по адресу http://{}", hyper_server_addr);
  
  let hyper_finish = hyper_server.with_graceful_shutdown(hyper_shutdown_signal());
  match hyper_finish.await {
    Err(e) => eprintln!("Ошибка сервера: {}", e),
    _ => println!("\nСервер успешно выключен.")
  }
}
