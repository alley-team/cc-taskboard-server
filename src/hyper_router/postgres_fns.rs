extern crate passwords;

use std::sync::Arc;
use tokio::sync::Mutex;
use super::data::{Token, TokenAuth, UserAuthData, ColorSet};
use tokio_postgres::{Client as PgClient, Error as PgError};
use passwords::{PasswordGenerator, hasher::{bcrypt, gen_salt}};
use chrono::Utc;

/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(cli: Arc<Mutex<PgClient>>) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
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

/// Регистрирует ключ.
///
/// WARNING: один ключ работает для одной регистрации. После регистрации ключ удаляется из БД.
pub async fn register_new_cc_key(cli: Arc<Mutex<PgClient>>) -> Result<String, PgError> {
  let mut cli = cli.lock().await;
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
  cli.execute("insert into cc_keys values ($1);", &[&key]).await?;
  Ok(key)
}

/// Проверяет наличие ключа в БД.
pub async fn check_cc_key(cli: Arc<Mutex<PgClient>>, some_key: String) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id = cli.query_one("select id from cc_keys where key = $1;", &[&some_key]).await?;
  Ok(id.get(0))
}

/// Удаляет ключ после использования.
pub async fn remove_cc_key(cli: Arc<Mutex<PgClient>>, key_id: i64) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  cli.execute("remove from cc_keys where id = $1;", &[&key_id]).await?;
  Ok(())
}

/// Создаёт пользователя.
/// 
/// Функция генерирует соль, хэширует пароль и соль - и записывает в базу данных. Возвращает идентификатор пользователя.
pub async fn create_user(
    cli: Arc<Mutex<PgClient>>,
    mut auth_data: UserAuthData,
) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  let salt = gen_salt();
  let pass = String::from_utf8(Vec::from(bcrypt(10, &salt, &auth_data.pass).unwrap())).unwrap();
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  auth_data.tokens.clear();
  let j = serde_json::to_string(&auth_data).unwrap();
  cli.execute("insert into users values (\"\", $1);", &[&j]).await?;
  Ok(id)
}

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(cli: Arc<Mutex<PgClient>>, id: i64) -> Result<String, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let auth_data = cli.query_one("select auth_data from users where id = $1;", &[&id]).await?;
  let mut auth_data: UserAuthData = serde_json::from_str(auth_data.get(0)).unwrap();
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
  let tk = pg.generate_one().unwrap();
  let from_dt = Utc::now();
  let token = Token { tk, from_dt };
  auth_data.tokens.push(token.clone());
  let j = serde_json::to_string(&auth_data).unwrap();
  cli.execute("update users set auth_data = $1 where id = $2", &[&j, &id]).await?;
  Ok(token.tk)
}

pub async fn create_page(
    cli: Arc<Mutex<PgClient>>,
    auth: TokenAuth,
    title: String,
    color_set: ColorSet,
) -> Result<i64, PgError> {
  Ok(0)
}
