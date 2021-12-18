use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Error as PgError;
use chrono::Utc;

use crate::sec::auth::{Token, TokenAuth, UserAuth, UserAuthData, RegisterUserData};
use crate::sec::key_gen;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(cli: PgClient) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let queries = vec![
    String::from("create table cc_keys (id bigserial, key varchar[64] unique);"),
    String::from("create table users (id bigserial, login varchar[64] unique, shared_pages varchar, auth_data varchar, apd varchar);"),
    String::from("create table pages (id bigserial, author bigint, title varchar[64], boards varchar, background_color char[7]);"),
    String::from("create table boards (id bigserial, author bigint, title varchar[64], tasks varchar, color char[7], background_color char[7]);"),
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
  cli.execute("insert into cc_keys values (default, $1);", &[&key]).await?;
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
  let (salt, salted_pass) = key_gen::salt_pass(register_data.pass.clone()).unwrap();
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let user_auth_data = UserAuthData { salt, salted_pass, tokens: vec![] };
  let user_auth_data = serde_json::to_string(&user_auth_data).unwrap();
  cli.execute("insert into users values (default, $2, '[]', $2, '{}');", &[&id, &register_data.login, &user_auth_data]).await?;
  Ok(id)
}

/// Возвращает идентификатор пользователя по логину и паролю.
pub async fn user_credentials_to_id(cli: PgClient, user_auth: UserAuth) -> Result<i64, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id: i64 = cli.query_one("select id from users where login = $1;",
                              &[&user_auth.login]).await?.get(0);
  let auth_data = cli.query_one("select auth_data from users where id = $1;", &[&id]).await?;
  let auth_data: UserAuthData = serde_json::from_str(auth_data.get(0)).unwrap();
  Ok(match key_gen::check_pass(auth_data.salt, auth_data.salted_pass, user_auth.pass) {
    true => id,
    false => -1,
  })
}

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
  let auth_data = serde_json::to_string(&auth_data).unwrap();
  cli.execute("update users set auth_data = $1 where id = $2;", &[&auth_data, &id]).await?;
  let ta = TokenAuth { id, token: token.tk };
  Ok(ta)
}

/// Получает все токены пользователя.
pub async fn get_all_tokens(cli: PgClient, id: i64) -> Result<Vec<Token>, PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let auth_data = cli.query_one("select auth_data from users where id = $1;", &[&id]).await?;
  let auth_data: UserAuthData = serde_json::from_str(auth_data.get(0)).unwrap();
  Ok(auth_data.tokens)
}

/// Обновляет все токены пользователя.
pub async fn write_all_tokens(cli: PgClient, id: i64, tokens: Vec<Token>) -> Result<(), PgError> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let auth_data = cli.query_one("select auth_data from users where id = $1;", &[&id]).await?;
  let mut auth_data: UserAuthData = serde_json::from_str(auth_data.get(0)).unwrap();
  auth_data.tokens = tokens;
  let auth_data = serde_json::to_string(&auth_data).unwrap();
  cli.execute("update users set auth_data = $1 where id = $2;", &[&auth_data, &id]).await?;
  Ok(())
}

/// Создаёт страницу.
pub async fn create_page(cli: PgClient, author: i64, title: String, background_color: String) -> Result<i64, PgError> {
  if title.is_empty() || background_color.bytes().count() != 7 || background_color.chars().nth(0) != Some('#') {
    return Ok(-1);
  }
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('pages', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  cli.execute("insert into pages values (default, $1, $2, '[]', $3);", &[&author, &title, &background_color]).await?;
  Ok(id)
}
