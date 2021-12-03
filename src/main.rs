use std::convert::Infallible;
use hyper::{Body, Server, Method};

use hyper::service::{make_service_fn, service_fn};
use hyper::server::conn::AddrStream;
use tokio_postgres::{NoTls, Error as PgError, Client as PgClient};
use hyper::http::{Request, Response, StatusCode, Result as HttpResult};
use std::{io, io::{Error as IOErr, ErrorKind as IOErrKind},
          error::Error as StdError, 
          // sync::{Arc, Mutex}, 
          boxed::Box, 
          net::SocketAddr};
use crate::data::{CCTaskboardAppContext, AdminAuth};

extern crate crypto;
extern crate passwords;

mod data, hyper_router;

/// 
fn setup() -> Result<(String, u16, String), Box<dyn StdError>> {
  let stdin = io::stdin();
  
  println!("Привет! Это сервер CC TaskBoard. Прежде чем мы начнём, заполните несколько параметров.");
  println!("Введите имя пользователя PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
  let pg_config = String::from("host=localhost user='") + &buffer + &String::from("' password='");
  
  println!("Введите пароль PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
  let pg_config = pg_config + &buffer + &String::from("'");
  
  println!("Введите номер порта сервера:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
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

#[cfg(test)]
mod tests {
  use super::*;
  
  #[should_panic(expected = "Не удалось подключиться к Postgres. Проверьте, запущен ли локальный сервер базы данных и создан ли там пользователь 'cc-taskboard-tests' с паролем 't5VU`m|WF^0q)QQFlDLpkot7'.")]
  async fn prepare_test() -> PgClient {
    let (mut pg_client, _) = pg_con("host=localhost user='cc-taskboard-tests' password='t5VU`m|WF^0q)QQFlDLpkot7'", NoTls).await.unwrap();
    tokio::spawn(async move {
      if let Err(e) = pg_connection.await {
        panic!("Ошибка подключения к PostgreSQL: {}", e);
      }
    });
    pg_client
  }
  
  async fn setup() -> Result<(), PgError> {
    let pg_client = prepare_test().await;
    
  }
}
