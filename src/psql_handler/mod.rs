use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Error as PgError;
use chrono::Utc;

use crate::hyper_router::auth::{Token, TokenAuth, UserAuthData, RegisterUserData};
use crate::hyper_router::data::{ColorSet};

mod key_gen;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(cli: PgClient) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let queries = vec![
    String::from("create table users (id bigserial, shared_pages varchar, auth_data varchar, apd varchar);"),
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
pub async fn register_new_cc_key(cli: PgClient) -> Result<String, PgError> {
  let mut cli = cli.lock().await;
  let key = key_gen::generate_strong(64).unwrap();
  cli.transaction().await?;
  cli.execute("insert into cc_keys values ($1);", &[&key]).await?;
  Ok(key)
}

/// Проверяет наличие ключа в БД.
pub async fn check_cc_key(cli: PgClient, some_key: String) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id = cli.query_one("select id from cc_keys where key = $1;", &[&some_key]).await?;
  Ok(id.get(0))
}

/// Удаляет ключ после использования.
pub async fn remove_cc_key(cli: PgClient, key_id: i64) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  cli.execute("remove from cc_keys where id = $1;", &[&key_id]).await?;
  Ok(())
}

/// Создаёт пользователя.
/// 
/// Функция генерирует соль, хэширует пароль и соль - и записывает в базу данных. Возвращает идентификатор пользователя.
pub async fn create_user(
    cli: PgClient,
    register_data: RegisterUserData,
) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  let (salt, salted_pass) = key_gen::salt_pass(register_data.pass.clone());
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let user_auth_data = UserAuthData { salt, salted_pass, tokens: vec![], ..register_data };
  let j = serde_json::to_string(&register_data).unwrap();
  cli.execute("insert into users values (\"\", $1, \"\");", &[&j]).await?;
  Ok(id)
}

///
pub async fn user_credentials_to_id(cli: PgClient

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(cli: PgClient, id: i64) -> Result<TokenAuth, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let auth_data = cli.query_one("select auth_data from users where id = $1;", &[&id]).await?;
  let mut auth_data: UserAuthData = serde_json::from_str(auth_data.get(0)).unwrap();
  let tk = key_gen::generate_strong(64).unwrap();
  let from_dt = Utc::now();
  let token = Token { tk, from_dt };
  auth_data.tokens.push(token.clone());
  let j = serde_json::to_string(&auth_data).unwrap();
  cli.execute("update users set auth_data = $1 where id = $2;", &[&j, &id]).await?;
  let ta = TokenAuth { id, token: token.tk };
  let ta = serde_json::to_string(&ta).unwrap();
  Ok(ta)
}

/// Создаёт страницу.
pub async fn create_page(
    cli: PgClient,
    auth: TokenAuth,
    title: String,
    color_set: ColorSet,
) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
}
