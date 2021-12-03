use tokio_postgres::{NoTls, Client as PgClient};

mod data;

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
    cont: CCTaskboardAppContext,
) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match hyper::body::to_bytes(req.into_body()).await {
      Err(_) => StatusCode::UNAUTHORIZED,
      Ok(bytes) => match json_parse_admin_auth_key(bytes).unwrap() == cont.admin_key {
        false => StatusCode::UNAUTHORIZED,
        true => match postgres_fns::db_setup(cli).await {
          Ok(_) => StatusCode::OK,
          Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
      }
    })
    .body(Body::empty())?)
}

/// Обрабатывает сигнал завершения работы сервера.
async fn hyper_shutdown_signal() {
  tokio::signal::ctrl_c()
    .await
    .expect("Не удалось установить комбинацию Ctrl+C как завершающую работу.");
}

fn ok_or_404(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_404(),
  }
}

/// Обрабатывает запросы клиентов.
///
/// 
async fn router(
    context: CCTaskboardAppContext,
    _addr: SocketAddr,
    req: Request<Body>
) -> Result<Response<Body>, Infallible> {
  let (pg_client, pg_connection) = tokio_postgres::connect(context.pg_config.as_str(), NoTls).await.unwrap();
  tokio::spawn(async move {
    if let Err(e) = pg_connection.await {
      eprintln!("Ошибка подключения к PostgreSQL: {}", e);
    }
  });
  Ok(match (req.method(), req.uri().path()) {
    (&Method::POST, "/pg-setup") => db_setup(req, pg_client, context).await.ok_or_404(),
    _ => route_404(),
  })
}
