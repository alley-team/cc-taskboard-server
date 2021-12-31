use std::sync::Arc;
use chrono::Utc;
use serde_json::Value as JsonValue;
use tokio::join;
use tokio::sync::Mutex;
use tokio_postgres::Error as PgError;

use crate::model::{Board, Card};
use crate::sec::auth::{Token, TokenAuth, SignInCredentials, SignUpCredentials, UserCredentials, AccountPlanDetails};
use crate::sec::key_gen;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;
type PgResult<T> = Result<T, PgError>;

/// Настраивает базу данных.
///
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
/// TODO Обработать все результаты выполнения запросов.
pub async fn db_setup(cli: PgClient) -> PgResult<u64> {
  let mut cli = cli.lock().await;
  let tr = cli.transaction().await?;
  let frs = join!(
    tr.execute("create table if not exists cc_keys (id bigserial, key varchar unique);", &[]),
    tr.execute("create table if not exists users (id bigserial, login varchar unique, shared_boards varchar, user_creds varchar, apd varchar);", &[]),
    tr.execute("create table if not exists boards (id bigserial, author bigint, shared_with varchar, title varchar, cards varchar, background_color varchar);", &[]),
    tr.execute("create table if not exists id_seqs (id varchar unique, val bigint);", &[]),
  );
  if frs.0.is_err() {
    return frs.0;
  };
  if frs.1.is_err() {
    return frs.1;
  };
  if frs.2.is_err() {
    return frs.2;
  };
  if frs.3.is_err() {
    return frs.3;
  };
  tr.commit().await?;
  Ok(0)
}

/// Регистрирует ключ.
///
/// WARNING: один ключ работает для одной регистрации. После регистрации ключ удаляется из БД.
pub async fn register_new_cc_key(cli: PgClient) -> PgResult<String> {
  let key = key_gen::generate_strong(64).unwrap();
  let mut cli = cli.lock().await;
  let tr = cli.transaction().await?;
  tr.execute("insert into cc_keys values (default, $1);", &[&key]).await?;
  tr.commit().await?;
  Ok(key)
}

/// Проверяет наличие ключа в БД.
pub async fn check_cc_key(cli: PgClient, some_key: &String) -> PgResult<i64> {
  let cli = cli.lock().await;
  let id = cli.query_one("select id from cc_keys where key = $1;", &[some_key]).await?;
  Ok(id.get(0))
}

/// Удаляет ключ после использования.
pub async fn remove_cc_key(cli: PgClient, key_id: &i64) -> PgResult<()> {
  let mut cli = cli.lock().await;
  let tr = cli.transaction().await?;
  tr.execute("delete from cc_keys where id = $1;", &[key_id]).await?;
  tr.commit().await?;
  Ok(())
}

/// Создаёт пользователя.
///
/// Функция генерирует соль, хэширует пароль и соль - и записывает в базу данных. Возвращает идентификатор пользователя.
pub async fn create_user(cli: PgClient, su_creds: &SignUpCredentials) -> PgResult<i64> {
  let (salt, salted_pass) = key_gen::salt_pass(su_creds.pass.clone()).unwrap();
  let mut cli = cli.lock().await;
  let tr = cli.transaction().await?;
  let id = tr.query_one("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let user_creds = UserCredentials { salt, salted_pass, tokens: vec![] };
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  let apd = AccountPlanDetails {
    billed_forever: false,
    payment_data: String::new(),
    is_paid_whenever: false,
    last_payment: Utc::now()
  };
  let apd = serde_json::to_string(&apd).unwrap();
  tr.execute("insert into users values ($1, $2, '[]', $3, $4);", &[&id, &su_creds.login, &user_creds, &apd]).await?;
  tr.commit().await?;
  Ok(id)
}

/// Возвращает идентификатор пользователя по логину и паролю.
pub async fn sign_in_creds_to_id(cli: PgClient, si_creds: &SignInCredentials) -> PgResult<i64> {
  let cli = cli.lock().await;
  let id: i64 = cli.query_one("select id from users where login = $1;",
                              &[&si_creds.login]).await?.get(0);
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[&id]).await?;
  let user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  Ok(match key_gen::check_pass(user_creds.salt, user_creds.salted_pass, &si_creds.pass) {
    true => id,
    false => -1,
  })
}

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(cli: PgClient, id: &i64) -> PgResult<TokenAuth> {
  let mut cli = cli.lock().await;
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  let tk = key_gen::generate_strong(64).unwrap();
  let from_dt = Utc::now();
  let token = Token { tk, from_dt };
  user_creds.tokens.push(token.clone());
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  let tr = cli.transaction().await?;
  tr.execute("update users set user_creds = $1 where id = $2;", &[&user_creds, id]).await?;
  tr.commit().await?;
  let ta = TokenAuth { id: *id, token: token.tk };
  Ok(ta)
}

/// Получает все токены пользователя.
pub async fn get_tokens_and_billing(cli: PgClient, id: &i64) -> PgResult<(Vec<Token>, AccountPlanDetails)> {
  let cli = cli.lock().await;
  let user_data = cli.query_one("select user_creds, apd from users where id = $1;", &[id]).await?;
  let user_creds: UserCredentials = serde_json::from_str(user_data.get(0)).unwrap();
  let billing: AccountPlanDetails = serde_json::from_str(user_data.get(1)).unwrap();
  Ok((user_creds.tokens, billing))
}

/// Обновляет все токены пользователя.
pub async fn write_tokens(cli: PgClient, id: &i64, tokens: &Vec<Token>) -> PgResult<()> {
  let mut cli = cli.lock().await;
  let user_creds = cli.query_one("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_creds: UserCredentials = serde_json::from_str(user_creds.get(0)).unwrap();
  user_creds.tokens = tokens.clone();
  let user_creds = serde_json::to_string(&user_creds).unwrap();
  let tr = cli.transaction().await?;
  tr.execute("update users set user_creds = $1 where id = $2;", &[&user_creds, id]).await?;
  tr.commit().await?;
  Ok(())
}

/// Создаёт доску.
pub async fn create_board(cli: PgClient, author: &i64, board: &Board) -> PgResult<i64> {
  if board.title.is_empty() || 
     board.background_color.bytes().count() != 7 || 
     board.background_color.chars().nth(0) != Some('#') {
    return Ok(-1);
  }
  let mut cli = cli.lock().await;
  let id = cli.query_one("select nextval(pg_get_serial_sequence('boards', 'id'));", &[]).await?;
  let id: i64 = id.get(0);
  let tr = cli.transaction().await?;
  tr.execute("insert into boards values ($1, $2, '[]', $3, '[]', $4);", &[&id, author, &board.title, &board.background_color]).await?;
  tr.commit().await?;
  Ok(id)
}

/// Удостоверяется, что пользователь имеет право получать содержимое этой доски.
pub async fn check_rights_on_board(cli: PgClient, user_id: &i64, board_id: &i64) -> PgResult<bool> {
  let cli = cli.lock().await;
  let shared_with = cli.query_one("select shared_with from boards where id = $1;", &[board_id]).await?;
  let shared_with: Vec<i64> = serde_json::from_str(shared_with.get(0)).unwrap();
  match shared_with.iter().position(|id| *id == *user_id) {
    None => Ok(false),
    Some(_) => Ok(true),
  }
}

/// Отдаёт доску пользователю.
pub async fn get_board(cli: PgClient, board_id: &i64) -> PgResult<Board> {
  let cli = cli.lock().await;
  let board_data = cli.query_one("select author, shared_with, title, cards, background_color from boards where id = $1;", &[board_id]).await?;
  let author: i64 = board_data.get(0);
  let shared_with: Vec<i64> = serde_json::from_str(board_data.get(1)).unwrap();
  let title: String = board_data.get(2);
  let cards: Vec<Card> = serde_json::from_str(board_data.get(3)).unwrap();
  let background_color: String = board_data.get(4);
  let board = Board { id: *board_id, author, shared_with, title, cards, background_color };
  Ok(board)
}

/// Применяет патч на доску.
pub async fn apply_patch_on_board(cli: PgClient, user_id: &i64, patch: &JsonValue) -> PgResult<bool> {
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
  let board_id = match patch.get("board_id").unwrap().as_i64() {
    None => return Ok(false),
    Some(v) => v,
  };
  let mut cli = cli.lock().await;
  let author_id = cli.query_one("select author from boards where id = $1;", &[&board_id]).await?;
  let author_id: i64 = author_id.get(0);
  if *user_id != author_id {
    return Ok(false);
  };
  let tr = cli.transaction().await?;
  if title_changed {
    let title: String = match patch.get("title").unwrap().as_str() {
      None => return Ok(false),
      Some(v) => String::from(v),
    };
    tr.execute("update boards set title = $1 where id = $2;", &[&title, &board_id]).await?;
  };
  if background_color_changed {
    let background_color: String = match patch.get("background_color").unwrap().as_str() {
      None => return Ok(false),
      Some(v) => String::from(v),
    };
    tr.execute("update boards set background_color = $1 where id = $2;", &[&background_color, &board_id]).await?;
  };
  tr.commit().await?;
  Ok(true)
}

/// Удаляет доску, если её автор - данный пользователь.
///
/// И обходит всех пользователей, удаляя у них id доски. Также удаляет последовательности идентификаторов.
pub async fn remove_board(cli: PgClient, user_id: &i64, board_id: &i64) -> PgResult<bool> {
  let mut cli = cli.lock().await;
  let author_id = cli.query_one("select author from boards where id = $1;", &[board_id]).await?;
  let author_id: i64 = author_id.get(0);
  if author_id != *user_id {
    return Ok(false);
  };
  let shared_with = cli.query_one("select shared_with from board where id = $1;", &[board_id]).await?;
  let shared_with: Vec<i64> = serde_json::from_str(shared_with.get(0)).unwrap();
  let tr = cli.transaction().await?;
  for user_id in shared_with.iter() {
    let shared_boards = tr.query_one("select shared_boards from users where id = $1;", &[&user_id]).await?;
    let mut shared_boards: Vec<i64> = serde_json::from_str(shared_boards.get(0)).unwrap();
    let this_board = shared_boards.iter().position(|id| *id == *board_id).unwrap();
    shared_boards.swap_remove(this_board);
    let shared_boards = serde_json::to_string(&shared_boards).unwrap();
    tr.execute("update users set shared_boards = $1 where id = $2;", &[&shared_boards, &user_id]).await?;
  };
  tr.execute("delete from boards where id = $1;", &[board_id]).await?;
  tr.execute("delete from id_seqs where id like concat($1, '%');", &[&board_id.to_string()]).await?;
  tr.commit().await?;
  Ok(true)
}

/// Подсчитывает все доски пользователя.
pub async fn count_boards(cli: PgClient, id: &i64) -> PgResult<usize> {
  let cli = cli.lock().await;
  let shared_boards = cli.query_one("select shared_boards from users where id = $1;", &[id]).await?;
  let shared_boards: JsonValue = serde_json::from_str(shared_boards.get(0)).unwrap();
  Ok(shared_boards.as_array().unwrap().len())
}

/// Проверяет, есть ли доступ у пользователя к данной доске.
pub async fn check_in_shared_with(cli: PgClient, user_id: &i64, board_id: &i64) -> PgResult<bool> {
  let cli = cli.lock().await;
  let shared_boards = cli.query_one("select shared_boards from users where id = $1;", &[user_id]).await?;
  let shared_boards: Vec<i64> = serde_json::from_str(shared_boards.get(0)).unwrap();
  if shared_boards.iter().position(|id| *id == *board_id).is_none() {
    return Ok(false);
  };
  let shared_with = cli.query_one("select shared_with from boards where id = $1;", &[board_id]).await?;
  let shared_with: Vec<i64> = serde_json::from_str(shared_with.get(0)).unwrap();
  match shared_with.iter().position(|id| *id == *user_id) {
    None => Ok(false),
    Some(_) => Ok(true),
  }
}

/// Добавляет карточку в доску.
///
/// WARNING Поскольку содержимое карточки валидируется при десериализации, его безопасно добавлять в базу данных. Но существует возможность добавления нескольких задач/подзадач с идентичными id, поэтому данная функция их переназначает.
/// WARNING Помимо этого, по причине авторства пользователя переназначаются идентификаторы авторов во всех вложенных задачах и подзадачах.
/// WARNING Функция не возвращает идентификаторы задач/подзадач, только id карточки.
pub async fn insert_card(cli: PgClient, user_id: &i64, board_id: &i64, mut card: Card) -> PgResult<i64> {
  let cards_id_seq = board_id.to_string();
  let mut cli = cli.lock().await;
  let mut next_card_id: i64 = match cli.query_one("select val from id_seqs where id = $1;", &[&cards_id_seq]).await {
    Err(_) => 1,
    Ok(res) => res.get(0),
  };
  let card_id = next_card_id;
  card.id = next_card_id;
  card.author = *user_id;
  let tasks_id_seq = cards_id_seq.clone() + "_" + &next_card_id.to_string();
  next_card_id += 1;
  // Все таски и сабтаски у нас новые, поэтому будем обходить их с новыми подпоследовательностями.
  let mut next_task_id: i64 = 1;
  let tr = cli.transaction().await?;
  for i in 0..card.tasks.len() {
    card.tasks[i].id = next_task_id;
    card.tasks[i].author = *user_id;
    let subtasks_id_seq = tasks_id_seq.clone() + "_" + &next_task_id.to_string();
    next_task_id += 1;
    let mut next_subtask_id: i64 = 1;
    for j in 0..card.tasks[i].subtasks.len() {
      card.tasks[i].subtasks[j].id = next_subtask_id;
      card.tasks[i].subtasks[j].author = *user_id;
      next_subtask_id += 1;
    };
    tr.execute("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", &[&subtasks_id_seq, &next_subtask_id]).await?;
  };
  tr.execute("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", &[&tasks_id_seq, &next_task_id]).await?;
  tr.execute("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", &[&cards_id_seq, &next_card_id]).await?;
  let cards = tr.query_one("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = match serde_json::from_str(cards.get(0)) {
    Err(_) => Vec::new(),
    Ok(v) => v,
  };
  cards.push(card);
  let cards = serde_json::to_string(&cards).unwrap();
  tr.execute("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await?;
  tr.commit().await?;
  Ok(card_id)
}

/// Применяет патч на карточку.
pub async fn apply_patch_on_card(cli: PgClient, user_id: &i64, patch: &JsonValue) -> PgResult<bool> {
  let mut title_changed: bool = false;
  if patch.get("title") != None {
    title_changed = true;
  };
  let mut background_color_changed: bool = false;
  if patch.get("background_color") != None {
    background_color_changed = true;
  };
  let mut text_color_changed: bool = false;
  if patch.get("text_color") != None {
    text_color_changed = true;
  };
  if !(title_changed || background_color_changed || text_color_changed) {
    return Ok(true);
  };
  let board_id = match patch.get("board_id").unwrap().as_i64() {
    None => return Ok(false),
    Some(v) => v,
  };
  let card_id = match patch.get("card_id").unwrap().as_i64() {
    None => return Ok(false),
    Some(v) => v,
  };
  let mut cli = cli.lock().await;
  let res = cli.query_one("select author, cards from boards where id = $1;", &[&board_id]).await?;
  let author_id: i64 = res.get(0);
  if *user_id != author_id {
    return Ok(false);
  };
  let mut cards: Vec<Card> = match serde_json::from_str(res.get(1)) {
    Err(_) => return Ok(false),
    Ok(v) => v,
  };
  let card_index: usize = match cards.iter().position(|c| (c.author == *user_id) && (c.id == card_id)) {
    None => return Ok(false),
    Some(v) => v,
  };
  if title_changed {
    cards[card_index].title = match patch.get("title").unwrap().as_str() {
      None => return Ok(false),
      Some(v) => String::from(v),
    };
  };
  if background_color_changed {
    cards[card_index].color_set.background_color = match patch.get("background_color").unwrap().as_str() {
      None => return Ok(false),
      Some(v) => String::from(v),
    };
  };
  if text_color_changed {
    cards[card_index].color_set.text_color = match patch.get("text_color").unwrap().as_str() {
      None => return Ok(false),
      Some(v) => String::from(v),
    };
  };
  let cards = serde_json::to_string(&cards).unwrap();
  let tr = cli.transaction().await?;
  tr.execute("update boards set cards = $1 where id = $2;", &[&cards, &board_id]).await?;
  tr.commit().await?;
  return Ok(true)
}

/// Удаляет карточку.
pub async fn remove_card(cli: PgClient, board_id: &i64, card_id: &i64) -> PgResult<bool> {
  let mut cli = cli.lock().await;
  let cards = cli.query_one("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = match serde_json::from_str(cards.get(0)) {
    Err(_) => return Ok(false),
    Ok(v) => v,
  };
  let card_index: usize = match cards.iter().position(|c| (c.id == *card_id)) {
    None => return Ok(false),
    Some(v) => v,
  };
  cards.remove(card_index);
  let cards = serde_json::to_string(&cards).unwrap();
  let tr = cli.transaction().await?;
  tr.execute("update boards set cards = $1 where id = $2;", &[&cards, &board_id]).await?;
  tr.commit().await?;
  return Ok(true)
}
