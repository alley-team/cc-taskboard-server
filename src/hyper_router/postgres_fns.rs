extern crate passwords;
use super::data::{/*Token, */UserAuthData};
use tokio_postgres::{Client as PgClient, Error as PgError};
use passwords::{PasswordGenerator, hasher::{bcrypt, gen_salt}};

/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(mut cli: PgClient) -> Result<(), PgError> {
  cli.transaction().await?;
  let queries = vec![
    String::from("create table users (id bigserial, shared_pages varchar, auth_data varchar);"),
    String::from("create table pages (id bigserial, title varchar[64], boards varchar, background_color char[7]);"),
    String::from("create table boards (id bigserial, title varchar[64], tasks varchar, color char[7], background_color char[7]);"),
    String::from("create table cc_keys (id bigserial, key varchar[64]);"),
    ];
  for x in &queries {
    cli.execute(x, &[]).await?;
  }
  Ok(())
}

pub async fn register_new_cc_key(mut cli: PgClient) -> Result<String, PgError> {
  let pg = PasswordGenerator {
    length: 64,
    numbers: true,
    lowercase_letters: true,
    uppercase_letters: true,
    symbols: true,
    strict: true,
    exclude_similar_characters: true,
    spaces: false,
  };
  let key = pg.generate_one().unwrap();
  cli.transaction().await?;
  cli.execute("insert into cc_keys values ($2);", &[&key]).await?;
  Ok(key)
}

/// Создаёт пользователя.
/// 
/// Функция генерирует соль, хэширует пароль и соль - и записывает в базу данных. Возвращает идентификатор пользователя.
pub async fn create_user(
  mut cli: PgClient,
  login: String,
  pass: String,
  cc_key: String,
) -> Result<i64, PgError> {
  let salt = gen_salt();
  let pass = String::from_utf8(Vec::from(bcrypt(10, &salt, &pass).unwrap())).unwrap();
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let auth_data = UserAuthData { login, pass, cc_key, tokens: vec![], };
  let j = serde_json::to_string(&auth_data).unwrap();
  cli.execute("insert into users values (\"\", $1);", &[&j]).await?;
  Ok(id)
}
