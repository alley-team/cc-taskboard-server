use std::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, Method, StatusCode};
use tokio_postgres;

fn app_get_all(s: String) -> String {
  
}

async fn hyper_routing(req: Request<Body>) -> Result<Response<Body>, Infallible> {
  let mut response = Response::new(Body::empty());
  match (req.method(), req.uri().path()) {
    (&Method::GET, "/") => {
      *response.body_mut() = Body::from(String::from("{ \"hello\": \"мир\" }"));
    },
    (&Method::POST, "/echo") => {
      *response.body_mut() = req.into_body();
    },
    _ => {
      *response.status_mut() = StatusCode::NOT_FOUND;
    },
  };
  Ok(response)
}

async fn hyper_shutdown_signal() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  println!("Привет! Это сервер CC TaskBoard. Прежде чем мы начнём, заполните несколько параметров.");
  println!("");
  let hyper_service = make_service_fn(|_conn| {
    async { Ok::<_, Infallible>(service_fn(hyper_routing)) }
  });
  let hyper_server_addr = ([127, 0, 0, 1], 9867).into();
  let hyper_server = Server::bind(&hyper_server_addr).serve(hyper_service);
  println!("Сервер слушает по адресу http://{}", hyper_server_addr);
  let (pg_client, pg_connection) = tokio_postgres::connect("host=localhost user=postgres", NoTls).await?;
  let hyper_finish = hyper_server.with_graceful_shutdown(hyper_shutdown_signal());
  if let Err(e) = hyper_finish.await {
    eprintln!("Ошибка сервера: {}", e);
  }
  Ok(())
}
