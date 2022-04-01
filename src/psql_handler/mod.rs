use bb8::Pool;
use bb8_postgres::PostgresConnectionManager as PgConManager;
use chrono::Utc;
use core::marker::{Send, Sync};
use custom_error::custom_error;
use futures::future;
use serde_json::Value as JsonValue;
use std::{boxed::Box, collections::HashSet};
use tokio_postgres::{ToStatement, types::ToSql, row::Row, NoTls};

use crate::model::{Board, BoardsShort, BoardHeader, Cards, Card, Task, Subtask, Tag, Timelines};
use crate::sec::auth::{Token, TokenAuth, SignInCredentials, SignUpCredentials, UserCredentials, AccountPlanDetails};
use crate::sec::key_gen;

type MResult<T> = Result<T, Box<dyn std::error::Error>>;

custom_error!{NFO{} = "Не удалось получить данные."}

/// Реализует операции ввода-вывода над пулом соединений с базой данных PostgreSQL.
#[derive(Clone)]
pub struct Db {
  pool: Pool<PgConManager<NoTls>>,
}

impl Db {
  /// Создаёт объект из пула соединений.
  pub fn new(pool: Pool<PgConManager<NoTls>>) -> Db {
    Db { pool }
  }

  /// Считывает одну строку из базы данных.
  pub async fn read<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> MResult<Row>
  where T: ?Sized + ToStatement {
    let cli = self.pool.get().await?;
    Ok(cli.query_one(statement, params).await?)
  }
  
  /// Записывает одно выражение в базу данных.
  pub async fn write<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> MResult<()>
  where T: ?Sized + ToStatement {
    let mut cli = self.pool.get().await?;
    let tr = cli.transaction().await?;
    tr.execute(statement, params).await?;
    tr.commit().await?;
    Ok(())
  }
  
  /// Считывает несколько значений по одной строке из базы данных.
  pub async fn read_mul<T>(&self, parts: Vec<(&T, Vec<&(dyn ToSql + Sync)>)>) -> MResult<Vec<Row>>
  where T: ?Sized + ToStatement + Send + Sync {
    let cli = self.pool.get().await?;
    let mut tasks = Vec::new();
    for i in 0..parts.len() {
      tasks.push(cli.query_one(parts[i].0, &parts[i].1));
    };
    let results = future::try_join_all(tasks).await?;
    Ok(results)
  }
  
  /// Записывает несколько значений в базу данных.
  pub async fn write_mul<T>(&self, parts: Vec<(&T, Vec<&(dyn ToSql + Sync)>)>) -> MResult<()>
  where T: ?Sized + ToStatement + Send + Sync {
    let mut cli = self.pool.get().await?;
    let tr = cli.transaction().await?;
    let mut tasks = Vec::new();
    for i in 0..parts.len() {
      tasks.push(tr.execute(parts[i].0, &parts[i].1));
    };
    future::try_join_all(tasks).await?;
    tr.commit().await?;
    Ok(())
  }
}

/// Настраивает базу данных.
///
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(db: &Db) -> MResult<()> {
  db.write_mul(vec![
    ("create table if not exists cc_keys (id bigserial, key varchar unique);", vec![]), 
    ("create table if not exists users (id bigserial, login varchar unique, shared_boards varchar, user_creds varchar, apd varchar);", vec![]), 
    ("create table if not exists boards (id bigserial, author bigint, shared_with varchar, header varchar, cards varchar, background_color varchar);", vec![]), 
    ("create table if not exists id_seqs (id varchar unique, val bigint);", vec![])
  ]).await
}

/// Регистрирует ключ.
///
/// Один ключ работает для одной регистрации. После регистрации ключ удаляется из БД.
pub async fn register_new_cc_key(db: &Db) -> MResult<String> {
  let key = key_gen::generate_strong(64)?;
  db.write("insert into cc_keys values (default, $1);", &[&key]).await?;
  Ok(key)
}

/// Проверяет наличие ключа в БД.
pub async fn check_cc_key(db: &Db, some_key: &String) -> MResult<i64> {
  Ok(db.read("select id from cc_keys where key = $1;", &[some_key]).await?.get(0))
}

/// Удаляет ключ после использования.
pub async fn remove_cc_key(db: &Db, key_id: &i64) -> MResult<()> {
  db.write("delete from cc_keys where id = $1;", &[key_id]).await
}

/// Создаёт пользователя.
///
/// Функция генерирует соль, хэширует пароль и соль - и записывает в базу данных. Возвращает идентификатор пользователя.
pub async fn create_user(db: &Db, sign_up_credentials: &SignUpCredentials) -> MResult<i64> {
  let (salt, salted_pass) = key_gen::salt_pass(sign_up_credentials.pass.clone())?;
  let id: i64 = db.read("select nextval(pg_get_serial_sequence('users', 'id'));", &[]).await?.get(0);
  let user_credentials = UserCredentials { salt, salted_pass, tokens: vec![] };
  let user_credentials = serde_json::to_string(&user_credentials)?;
  let billing = AccountPlanDetails {
    billed_forever: false,
    payment_data: String::new(),
    is_paid_whenever: false,
    last_payment: Utc::now()
  };
  let billing = serde_json::to_string(&billing)?;
  db.write("insert into users values ($1, $2, '[]', $3, $4);", &[&id, &sign_up_credentials.login, &user_credentials, &billing]).await?;
  Ok(id)
}

/// Возвращает идентификатор пользователя по логину и паролю.
pub async fn sign_in_creds_to_id(db: &Db, sign_in_credentials: &SignInCredentials) -> MResult<i64> {
  custom_error!{IncorrectPassword{} = "Неверный пароль!"};
  let id_and_credentials = db.read("select id, user_creds from users where login = $1);", &[&sign_in_credentials.login]).await?;
  let user_credentials: UserCredentials = serde_json::from_str(id_and_credentials.get(1))?;
  match key_gen::check_pass(
    user_credentials.salt,
    user_credentials.salted_pass,
    &sign_in_credentials.pass
  ) {
    true => Ok(id_and_credentials.get(0)),
    _ => Err(Box::new(IncorrectPassword {})),
  }
}

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(db: &Db, id: &i64) -> MResult<TokenAuth> {
  let user_credentials = db.read("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_credentials: UserCredentials = serde_json::from_str(user_credentials.get(0))?;
  let token = Token {
    tk: key_gen::generate_strong(64)?,
    from_dt: Utc::now(),
  };
  user_credentials.tokens.push(token.clone());
  let user_credentials = serde_json::to_string(&user_credentials)?;
  db.write("update users set user_creds = $1 where id = $2;", &[&user_credentials, id]).await?;
  let token_auth = TokenAuth { id: *id, token: token.tk };
  Ok(token_auth)
}

/// Получает все токены пользователя.
pub async fn get_tokens_and_billing(db: &Db, id: &i64) -> MResult<(Vec<Token>, AccountPlanDetails)> {
  let user_data = db.read("select user_creds, apd from users where id = $1;", &[id]).await?;
  let user_credentials: UserCredentials = serde_json::from_str(user_data.get(0))?;
  let billing: AccountPlanDetails = serde_json::from_str(user_data.get(1))?;
  Ok((user_credentials.tokens, billing))
}

/// Обновляет все токены пользователя.
pub async fn write_tokens(db: &Db, id: &i64, tokens: &Vec<Token>) -> MResult<()> {
  let user_credentials = db.read("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_credentials: UserCredentials = serde_json::from_str(user_credentials.get(0))?;
  user_credentials.tokens = tokens.clone();
  let user_credentials = serde_json::to_string(&user_credentials)?;
  db.write("update users set user_creds = $1 where id = $2;", &[&user_credentials, id]).await
}

/// Отдаёт список досок пользователя.
pub async fn list_boards(db: &Db, id: &i64) -> MResult<String> {
  let boards = db.read("select shared_boards from users where id = $1;", &[id]).await?;
  let boards: Vec<i64> = serde_json::from_str(boards.get(0))?;
  let mut shorts: Vec<BoardsShort> = vec![];
  boards.iter().for_each(|v| {
    let header = db.read("select header from boards where id = $1;", &[&v]).await?;
    let short = BoardsShort {
      id: v,
      
    };
  });
}

/// Создаёт доску.
pub async fn create_board(db: &Db, author: &i64, board: &Board) -> MResult<i64> {
  custom_error!{IncorrectBoard
    EmptyTitle = "У доски пустой заголовок.",
    IncompatibleColorLen = "Цвет не представлен в виде #RRGGBB.",
    IncompatibleColorBeginning = "Цвет не начинается с #."
  };
  if board.header.title.is_empty() { return Err(Box::new(IncorrectBoard::EmptyTitle)); };
  if board.background_color.bytes().count() != 7 || 
     board.header.header_background_color.bytes().count() != 7 ||
     board.header.header_text_color.bytes().count() != 7 {
    return Err(Box::new(IncorrectBoard::IncompatibleColorLen));
  };
  if board.background_color.chars().nth(0) != Some('#') ||
     board.header.header_background_color.chars().nth(0) != Some('#') ||
     board.header.header_text_color.chars().nth(0) != Some('#') {
    return Err(Box::new(IncorrectBoard::IncompatibleColorBeginning));
  };
  let data = db.read_mul(vec![
    ("select nextval(pg_get_serial_sequence('boards', 'id'));", vec![]),
    ("select shared_boards from users where id = $1;", vec![author])
  ]).await?;
  let id: i64 = data[0].get(0);
  let mut shared_boards = serde_json::from_str::<Vec<i64>>(data[1].get(0))?;
  shared_boards.push(id);
  let shared_with = vec![*author];
  let shared_with = serde_json::to_string(&shared_with)?;
  let shared_boards = serde_json::to_string(&shared_boards)?;
  let header = serde_json::to_string(&board.header)?;
  let board_queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    (
      "insert into boards values ($1, $2, $3, $4, '[]', $5);",
      vec![&id, author, &shared_with, &header, &board.background_color]
    ),
    ("update users set shared_boards = $1 where id = $2;", vec![&shared_boards, author])
  ];
  db.write_mul(board_queries).await?;
  Ok(id)
}

/// Отдаёт доску пользователю.
pub async fn get_board(db: &Db, board_id: &i64) -> MResult<String> {
  let board_data = db.read("select author, shared_with, header, cards, background_color from boards where id = $1;", &[board_id]).await?;
  let author: i64 = board_data.get(0);
  let shared_with: String = board_data.get(1);
  let header: String = board_data.get(2);
  let cards: String = board_data.get(3);
  let background_color: String = board_data.get(4);
  Ok(format!(r#"{{"id":{},"author":{},"shared_with":{},"header":{},"cards":{},"background_color":"{}"}}"#, *board_id, author, shared_with, header, cards, background_color))
}

/// Применяет патч на доску.
pub async fn apply_patch_on_board(db: &Db, user_id: &i64, board_id: &i64, patch: &JsonValue)
  -> MResult<()>
{
  custom_error!{NTA{} = "Пользователь не может редактировать доску."};
  let author_id_and_header = db.read("select author, header from boards where id = $1;", &[board_id]).await?;
  let author_id: i64 = author_id_and_header.get(0);
  if *user_id != author_id { return Err(Box::new(NTA{})); };
  let header: String = author_id_and_header.get(1);
  let mut header: BoardHeader = serde_json::from_str(&header)?;
  if let Some(title) = patch.get("title") {
    let title = String::from(title.as_str().ok_or(NFO{})?);
    header.title = title;
  };
  if let Some(background_color) = patch.get("background_color") {
    let background_color = String::from(background_color.as_str().ok_or(NFO{})?);
    let r: Vec<&(dyn ToSql + Sync)> = vec![&background_color, board_id];
    db.write("update boards set background_color = $1 where id = $2;", &r).await?;
  };
  if let Some(header_background_color) = patch.get("header_background_color") {
    let header_background_color = String::from(header_background_color.as_str().ok_or(NFO{})?);
    header.header_background_color = header_background_color;
  };
  if let Some(header_text_color) = patch.get("header_text_color") {
    let header_text_color = String::from(header_text_color.as_str().ok_or(NFO{})?);
    header.header_text_color = header_text_color;
  };
  let header = serde_json::to_string(&header)?;
  let r: Vec<&(dyn ToSql + Sync)> = vec![&header, board_id];
  db.write("update boards set header = $1 where id = $2;", &r).await?;
  Ok(())
}

/// Удаляет доску, если её автор - данный пользователь.
///
/// И обходит всех пользователей, удаляя у них id доски. Также удаляет последовательности идентификаторов.
pub async fn remove_board(db: &Db, user_id: &i64, board_id: &i64) -> MResult<()> {
  custom_error!{NTA{} = "Пользователь не может редактировать доску."};
  let author_id_and_shared_with = db.read("select author, shared_with from boards where id = $1;", &[board_id]).await?;
  let author_id: i64 = author_id_and_shared_with.get(0);
  if author_id != *user_id { return Err(Box::new(NTA{})); };
  let shared_with: Vec<i64> = serde_json::from_str(author_id_and_shared_with.get(1))?;
  let mut shared_boards_queries = Vec::new();
  shared_with.iter().for_each(|v| {
    let r: Vec<&(dyn ToSql + Sync)> = vec![v];
    shared_boards_queries.push(("select shared_boards from users where id = $1;", r));
  });
  let shared_boards: Vec<Vec<i64>> = db.read_mul(shared_boards_queries).await?
                                       .iter()
                                       .map(|v| { serde_json::from_str::<Vec<i64>>(v.get(0)).unwrap() })
                                       .collect();
  let ids_and_shared_boards: Vec<(i64, Vec<i64>)> = shared_with.into_iter()
                                                      .zip(shared_boards.into_iter())
                                                      .collect();
  let mut tasks = Vec::new();
  for i in 0..ids_and_shared_boards.len() {
    let board_id = *board_id;
    let pair = (ids_and_shared_boards[i].0, ids_and_shared_boards[i].1.clone());
    let task = tokio::task::spawn(async move {
      let user_id = pair.0;
      let mut shared_boards = pair.1;
      let this_board = shared_boards.iter().position(|id| *id == board_id).ok_or(NFO{})?;
      shared_boards.swap_remove(this_board);
      let shared_boards = serde_json::to_string(&shared_boards)?;
      Result::<(String, i64), Box<dyn std::error::Error + Send + Sync>>::Ok((shared_boards, user_id))
    });
    tasks.push(task);
  };
  let results = future::try_join_all(tasks).await?;
  let mut _results = Vec::new();
  for i in 0..results.len() {
    _results.push(results[i].as_ref().unwrap());
  };
  let results: Vec<&(String, i64)> = _results;
  let mut shared_boards_queries = Vec::new();
  for i in 0..results.len() {
    let r: Vec<&(dyn ToSql + Sync)> = vec![&results[i].0, &results[i].1];
    shared_boards_queries.push(("update users set shared_boards = $1 where id = $2;", r));
  };
  shared_boards_queries.push(("delete from boards where id = $1;", vec![board_id]));
  shared_boards_queries.push((
    "delete from id_seqs where id like concat(cast($1 as varchar), '%');",
    vec![board_id]
  ));
  db.write_mul(shared_boards_queries).await
}

/// Подсчитывает все доски пользователя.
pub async fn count_boards(db: &Db, id: &i64) -> MResult<usize> {
  Ok(
    serde_json::from_str::<JsonValue>(
      db.read("select shared_boards from users where id = $1;", &[id])
        .await?
        .get(0)
    )?.as_array()
      .ok_or(NFO{})?
      .len())
}

/// Проверяет, есть ли доступ у пользователя к данной доске.
pub async fn in_shared_with(db: &Db, user_id: &i64, board_id: &i64) -> MResult<()> {
  let mut iter = db.read_mul(vec![
    ("select shared_boards from users where id = $1;", vec![user_id]),
    ("select shared_with from boards where id = $1;", vec![board_id]),
  ]).await?
    .into_iter()
    .map(|v| { serde_json::from_str::<Vec<i64>>(v.get(0)).unwrap() });
  match iter.next().ok_or(NFO{})?.iter().position(|id| *id == *board_id).is_some() && 
        iter.next().ok_or(NFO{})?.iter().position(|id| *id == *user_id).is_some() {
    false => Err(Box::new(NFO{})),
    _ => Ok(()),
  }
}

/// Добавляет карточку в доску.
///
/// Поскольку содержимое карточки валидируется при десериализации, его безопасно добавлять в базу данных. Но существует возможность добавления нескольких задач/подзадач с идентичными id, поэтому данная функция их переназначает. Помимо этого, по причине авторства пользователя переназначаются идентификаторы авторов во всех вложенных задачах и подзадачах.
///
/// Функция не возвращает идентификаторы задач/подзадач, только id карточки.
pub async fn insert_card(db: &Db, user_id: &i64, board_id: &i64, mut card: Card) -> MResult<i64> {
  let cards_id_seq = board_id.to_string();
  let mut next_card_id: i64 = match db.read("select val from id_seqs where id = $1;", &[&cards_id_seq]).await {
    Ok(res) => res.get(0),
    _ => 1,
  };
  let card_id = next_card_id;
  card.id = next_card_id;
  card.author = *user_id;
  let tasks_id_seq = cards_id_seq.clone() + "_" + &next_card_id.to_string();
  next_card_id += 1;
  // Все таски и сабтаски у нас новые, поэтому будем обходить их с новыми подпоследовательностями.
  let mut next_task_id: i64 = 1;
  let shared_with = db.read("select shared_with from boards where id = $1;", &[board_id]).await?;
  let shared_with: Vec<i64> = serde_json::from_str(shared_with.get(0))?;
  let shared_with: HashSet<i64> = shared_with.into_iter().collect();
  let mut id_seqs_queries_data: Vec<(String, i64)> = Vec::new();
  for i in 0..card.tasks.len() {
    card.tasks[i].id = next_task_id;
    card.tasks[i].author = *user_id;
    let subtasks_id_seq = tasks_id_seq.clone() + "_" + &next_task_id.to_string();
    next_task_id += 1;
    let mut executors: Vec<i64> = Vec::new();
    card.tasks[i].executors.iter().filter(|e| shared_with.contains(e)).for_each(|i| executors.push(*i));
    card.tasks[i].executors = executors;
    let mut next_subtask_id: i64 = 1;
    for j in 0..card.tasks[i].subtasks.len() {
      card.tasks[i].subtasks[j].id = next_subtask_id;
      card.tasks[i].subtasks[j].author = *user_id;
      next_subtask_id += 1;
      let mut executors: Vec<i64> = Vec::new();
      card.tasks[i].subtasks[j].executors
                               .iter()
                               .filter(|e| shared_with.contains(e))
                               .for_each(|i| executors.push(*i));
      card.tasks[i].subtasks[j].executors = executors;
    };
    id_seqs_queries_data.push((subtasks_id_seq, next_subtask_id));
  };
  id_seqs_queries_data.push((tasks_id_seq, next_task_id));
  id_seqs_queries_data.push((cards_id_seq, next_card_id));
  let mut id_seqs_queries = Vec::new();
  let query = "insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;";
  for i in 0..id_seqs_queries_data.len() {
    let r: Vec<&(dyn ToSql + Sync)> = vec![&id_seqs_queries_data[i].0, &id_seqs_queries_data[i].1];
    id_seqs_queries.push((query, r));
  };
  db.write_mul(id_seqs_queries).await?;
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = match serde_json::from_str(cards.get(0)) {
    Ok(v) => v,
    _ => Vec::new(),
  };
  cards.push(card);
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await?;
  Ok(card_id)
}

/// Применяет патч на карточку.
pub async fn apply_patch_on_card(db: &Db, board_id: &i64, card_id: &i64, patch: &JsonValue)
  -> MResult<()>
{
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let card = cards.get_mut_card(card_id)?;
  if let Some(title) = patch.get("title") {
    card.title = String::from(title.as_str().ok_or(NFO{})?);
  };
  if let Some(background_color) = patch.get("background_color") {
    card.background_color = String::from(background_color.as_str().ok_or(NFO{})?);
  };
  if let Some(header_text_color) = patch.get("header_text_color") {
    card.header_text_color = String::from(header_text_color.as_str().ok_or(NFO{})?);
  };
  if let Some(header_background_color) = patch.get("header_background_color") {
    card.header_background_color = String::from(header_background_color.as_str().ok_or(NFO{})?);
  };
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Удаляет карточку.
pub async fn remove_card(db: &Db, board_id: &i64, card_id: &i64) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.remove_card(card_id)?;
  let cards = serde_json::to_string(&cards)?;
  let tasks_id_seq = board_id.to_string() + "_" + &card_id.to_string() + "%";
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("delete from id_seqs where id like $1;", vec![&tasks_id_seq]),
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
  ];
  db.write_mul(queries).await
}

/// Создаёт задачу.
pub async fn insert_task(db: &Db, user_id: &i64, board_id: &i64, card_id: &i64, mut task: Task) 
  -> MResult<i64> 
{
  let tasks_id_seq = board_id.to_string() + "_" + &card_id.to_string();
  let data = db.read("select cards, shared_with from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(data.get(0))?;
  let shared_with: Vec<i64> = serde_json::from_str(data.get(1))?;
  let shared_with: HashSet<i64> = shared_with.into_iter().collect();
  let mut next_task_id: i64 = match db.read("select val from id_seqs where id = $1;", &[&tasks_id_seq]).await {
    Ok(res) => res.get(0),
    _ => 1,
  };
  task.id = next_task_id;
  let task_id = next_task_id;
  task.author = *user_id;
  next_task_id += 1;
  let mut executors: Vec<i64> = Vec::new();
  task.executors.iter().filter(|e| shared_with.contains(e)).for_each(|i| executors.push(*i));
  task.executors = executors;
  let subtasks_id_seq = tasks_id_seq.clone() + "_" + &next_task_id.to_string();
  let mut next_subtask_id: i64 = 1;
  for i in 0..task.subtasks.len() {
    task.subtasks[i].id = next_subtask_id;
    task.subtasks[i].author = *user_id;
    next_subtask_id += 1;
    let mut executors: Vec<i64> = Vec::new();
    task.subtasks[i].executors.iter().filter(|e| shared_with.contains(e)).for_each(|i| executors.push(*i));
    task.subtasks[i].executors = executors;
  };
  cards.get_mut_card(card_id)?.tasks.push(task);
  let cards = serde_json::to_string(&cards)?;
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
    ("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", vec![&subtasks_id_seq, &next_subtask_id]),
    ("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", vec![&tasks_id_seq, &next_task_id]),
  ];
  db.write_mul(queries).await?;
  Ok(task_id)
}

/// Применяет патч на задачу.
pub async fn apply_patch_on_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  patch: &JsonValue
) -> MResult<()> {
  let data = db.read("select cards, shared_with from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(data.get(0))?;
  let task = cards.get_mut_task(card_id, task_id)?;
  if let Some(title) = patch.get("title") {
    task.title = String::from(title.as_str().ok_or(NFO{})?);
  };
  if let Some(executors) = patch.get("executors") {
    let shared_with: Vec<i64> = serde_json::from_str(data.get(1))?;
    let shared_with: HashSet<i64> = shared_with.into_iter().collect();
    let executors: Vec<i64> = serde_json::from_value(executors.clone())?;
    task.executors = Vec::new();
    executors.iter()
             .filter(|e| shared_with.contains(e))
             .for_each(|i| task.executors.push(*i));
  };
  if let Some(exec) = patch.get("exec") {
    task.exec = exec.as_bool().ok_or(NFO{})?;
  };
  if let Some(notes) = patch.get("notes") {
    task.notes = String::from(notes.as_str().ok_or(NFO{})?);
  };
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Удаляет задачу.
pub async fn remove_task(db: &Db, board_id: &i64, card_id: &i64, task_id: &i64)
  -> MResult<()>
{
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.remove_task(card_id, task_id)?;
  let cards = serde_json::to_string(&cards)?;
  let subtasks_id_seq = board_id.to_string() + "_" + &card_id.to_string() + "_" + &task_id.to_string();
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("delete from id_seqs where id = $1;", vec![&subtasks_id_seq]),
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
  ];
  db.write_mul(queries).await
}

/// Устанавливает метки на задачу.
pub async fn set_tags_on_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  tags: &Vec<Tag>,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.get_mut_task(card_id, task_id)?.tags = tags.to_vec();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Устанавливает временные рамки на задачу.
pub async fn set_timelines_on_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  timelines: &Timelines,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.get_mut_task(card_id, task_id)?.timelines = timelines.clone();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Создаёт подзадачу.
pub async fn insert_subtask(
  db: &Db,
  user_id: &i64,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  mut subtask: Subtask,
) -> MResult<i64> {
  let subtasks_id_seq = board_id.to_string() + "_" + &card_id.to_string() + "_" + &task_id.to_string();
  let data = db.read("select cards, shared_with from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(data.get(0))?;
  let shared_with: Vec<i64> = serde_json::from_str(data.get(1))?;
  let shared_with: HashSet<i64> = shared_with.into_iter().collect();
  let mut next_subtask_id: i64 = match db.read("select val from id_seqs where id = $1;", &[&subtasks_id_seq]).await {
    Ok(res) => res.get(0),
    _ => 1,
  };
  subtask.id = next_subtask_id;
  let subtask_id = next_subtask_id;
  subtask.author = *user_id;
  next_subtask_id += 1;
  let mut executors: Vec<i64> = Vec::new();
  subtask.executors.iter().filter(|e| shared_with.contains(e)).for_each(|i| executors.push(*i));
  subtask.executors = executors;
  cards.get_mut_task(card_id, task_id)?.subtasks.push(subtask);
  let cards = serde_json::to_string(&cards)?;
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
    ("insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;", vec![&subtasks_id_seq, &next_subtask_id]),
  ];
  db.write_mul(queries).await?;
  Ok(subtask_id)
}

/// Применяет патч на подзадачу.
pub async fn apply_patch_on_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  patch: &JsonValue,
) -> MResult<()> {
  let data = db.read("select cards, shared_with from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(data.get(0))?;
  let subtask = cards.get_mut_subtask(card_id, task_id, subtask_id)?;
  if let Some(title) = patch.get("title") {
    subtask.title = String::from(title.as_str().ok_or(NFO{})?);
  };
  if let Some(executors) = patch.get("executors") {
    let shared_with: Vec<i64> = serde_json::from_str(data.get(1))?;
    let shared_with: HashSet<i64> = shared_with.into_iter().collect();
    let executors: Vec<i64> = serde_json::from_value(executors.clone())?;
    subtask.executors = Vec::new();
    executors.iter()
             .filter(|e| shared_with.contains(e))
             .for_each(|i| subtask.executors.push(*i));
  };
  if let Some(exec) = patch.get("exec") {
    subtask.exec = exec.as_bool().ok_or(NFO{})?;
  };
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Удаляет подзадачу.
pub async fn remove_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.remove_subtask(card_id, task_id, subtask_id)?;
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Устанавливает метки на подзадачу.
pub async fn set_tags_on_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  tags: &Vec<Tag>,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags = tags.to_vec();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Устанавливает временные рамки на подзадачу.
pub async fn set_timelines_on_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  timelines: &Timelines,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  cards.get_mut_subtask(card_id, task_id, subtask_id)?.timelines = timelines.clone();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}
