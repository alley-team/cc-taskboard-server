use std::convert::Infallible;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, Method, StatusCode};

async fn routing(req: Request<Body>) -> Result<Response<Body>, Infallible> {
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

async fn shutdown_signal() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
  let make_svc = make_service_fn(|_conn| {
    async { Ok::<_, Infallible>(service_fn(routing)) }
  });
  let addr = ([127, 0, 0, 1], 9867).into();
  let server = Server::bind(&addr).serve(make_svc);
  println!("Сервер слушает по адресу http://{}", addr);
  let graceful = server.with_graceful_shutdown(shutdown_signal());
  if let Err(e) = graceful.await {
    eprintln!("Ошибка сервера: {}", e);
  }
  Ok(())
}
