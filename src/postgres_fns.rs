/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
async fn db_setup(mut cli: tokio_postgres::Client) -> Result<(), tokio_postgres::Error> {
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
