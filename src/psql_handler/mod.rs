use std::sync::Arc;
use chrono::Utc;
use serde_json::Value as JsonValue;
use tokio::join;
use tokio::sync::Mutex;
use tokio_postgres::Error as PgError;

use crate::model::Board;
use crate::sec::auth::{Token, TokenAuth, SignInCredentials, SignUpCredentials, UserCredentials, AccountPlanDetails};
use crate::sec::key_gen;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;
type PgResult<T> = Result<T, PgError>;

/// Настраивает базу данных.
/// 
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
/// TODO Обработать все результаты выполнения запросов.
pub async fn db_setup(cli: PgClient) -> PgResult<()> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  join!(
      cli.execute("create table cc_keys (id bigserial, key varchar[64] unique);", &[]),
      cli.execute("create table users (id bigserial, login varchar[64] unique, shared_boards varchar, user_creds varchar, apd varchar);", &[]),
      cli.execute("create table boards (id bigserial, author bigint, title varchar[64], cards varchar, background_color char[7]);", &[]),
  );
  Ok(())
}

/// Регистрирует ключ.
///
/// WARNING: один ключ работает для одной регистрации. После регистрации ключ удаляется из БД.
pub async fn register_new_cc_key(cli: PgClient) -> PgResult<String> {
  let mut cli = cli.lock().await;
  let key = key_gen::generate_strong(64).unwrap();
  cli.transaction().await?;
  cli.execute("insert into cc_keys values (default, $1);", &[&key]).await?;
  Ok(key)
}

/// Проверяет наличие ключа в БД.
pub async fn check_cc_key(cli: PgClient, some_key: String) -> PgResult<i64> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id = cli.query_one("select id from cc_keys where key = $1;", &[&some_key]).await?;
  Ok(id.get(0))
}

/// Удаляет ключ после использования.
pub async fn remove_cc_key(cli: PgClient, key_id: i64) -> PgResult<()> {
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
    su_creds: SignUpCredentials,
) -> PgResult<i64> {
  let mut cli = cli.lock().await;
  let (salt, salted_pass) = key_gen::salt_pass(su_creds.pass.clone()).unwrap();
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let user_creds = UserCredentials { salt, salted_pass, tokens: vec![] };
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  cli.execute("insert into users values (default, $2, '[]', $2, '{}');", &[&id, &su_creds.login, &user_creds]).await?;
  Ok(id)
}

/// Возвращает идентификатор пользователя по логину и паролю.
pub async fn sign_in_creds_to_id(cli: PgClient, si_creds: SignInCredentials) -> PgResult<i64> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id: i64 = cli.query_one("select id from users where login = $1;",
                              &[&si_creds.login]).await?.get(0);
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[&id]).await?;
  let user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  Ok(match key_gen::check_pass(user_creds.salt, user_creds.salted_pass, si_creds.pass) {
    true => id,
    false => -1,
  })
}

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(cli: PgClient, id: i64) -> PgResult<TokenAuth> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[&id]).await?;
  let mut user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  let tk = key_gen::generate_strong(64).unwrap();
  let from_dt = Utc::now();
  let token = Token { tk, from_dt };
  user_creds.tokens.push(token.clone());
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  cli.execute("update users set user_creds = $1 where id = $2;", &[&user_creds, &id]).await?;
  let ta = TokenAuth { id, token: token.tk };
  Ok(ta)
}

/// Получает все токены пользователя.
pub async fn get_tokens_and_billing(cli: PgClient, id: i64) 
    -> PgResult<(Vec<Token>, AccountPlanDetails)> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let user_data = cli.query_one("select user_creds, apd from users where id = $1;", &[&id]).await?;
  let user_creds: UserCredentials = serde_json::from_str(user_data.get(0)).unwrap();
  let billing: AccountPlanDetails = serde_json::from_str(user_data.get(1)).unwrap();
  Ok((user_creds.tokens, billing))
}

/// Обновляет все токены пользователя.
pub async fn write_tokens(cli: PgClient, id: i64, tokens: Vec<Token>) -> PgResult<()> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[&id]).await?;
  let mut user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  user_creds.tokens = tokens;
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  cli.execute("update users set user_creds = $1 where id = $2;", &[&user_creds, &id]).await?;
  Ok(())
}

/// Создаёт доску.
pub async fn create_board(cli: PgClient, author: i64, board: Board) -> PgResult<i64> {
  if board.title.is_empty() || 
     board.background_color.bytes().count() != 7 || 
     board.background_color.chars().nth(0) != Some('#') {
    return Ok(-1);
  }
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('boards', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  cli.execute("insert into boards values (default, $1, $2, '[]', $3);", &[&author, &board.title, &board.background_color]).await?;
  Ok(id)
}

/// Применяет патч на доску.
pub async fn apply_patch_on_board(cli: PgClient, user_id: i64, patch: JsonValue) -> PgResult<bool> {
  let mut title_changed: bool = false;
  if patch.get("title") != None {
    title_changed = true;
  };
  let mut background_color_changed: bool = false;
  if patch.get("background_color") != None {
    background_color_changed = true;
  };
  if !(title_changed || background_color_changed) {
    return Ok(true);
  };
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let board_id = match patch.get("board_id").unwrap().as_i64() {
    None => return Ok(false),
    Some(v) => v,
  };
  let author_id = cli.query_one("select author from boards where id = $1;", &[&board_id]).await?;
  let author_id: i64 = author_id.get(0);
  if user_id != author_id {
    return Ok(false);
  };
  if title_changed {
    let title = String::from(patch.get("title").unwrap().as_str().unwrap());
    cli.execute("update boards set title = $1 where id = $2;", &[&title, &board_id]).await?;
  };
  if background_color_changed {
    let background_color = String::from(patch.get("background_color").unwrap().as_str().unwrap());
    cli.execute("update boards set background_color = $1 where id = $2;", &[&background_color, &board_id]).await?;
  };
  Ok(true)
}

/// Удаляет доску.
/// 
/// И обходит всех пользователей, удаляя у них id доски.
pub async fn remove_board(cli: PgClient, board_id: i64) -> PgResult<()> {
  Ok(())
}

/// Подсчитывает все доски пользователя.
pub async fn count_boards(cli: PgClient, id: i64) -> PgResult<usize> {
  let mut cli = cli.lock().await;
  cli.transaction().await?;
  let shared_boards = cli.query_one("select shared_boards from users where id = $1;", &[&id]).await?;
  let shared_boards: serde_json::Value = serde_json::from_str(shared_boards.get(0)).unwrap();
  Ok(shared_boards.as_array().unwrap().len())
}
