//! Отвечает за реализацию логики приложения.

use chrono::Utc;
use custom_error::custom_error;
use futures::future;
use serde_json::Value as JsonValue;
use sha3::{Digest, Sha3_256};
use std::collections::HashSet;
use tokio_postgres::types::ToSql;

use crate::model::{Board, BoardsShort, BoardHeader, BoardBackground, Cards, Card, Task, Subtask, Tag, Timelines};
use crate::psql_handler::Db;
use crate::sec::auth::{Token, TokenAuth, SignInCredentials, SignUpCredentials, UserCredentials, AccountPlanDetails};
use crate::sec::color_vld::validate_color;
use crate::sec::key_gen;

type MResult<T> = Result<T, Box<dyn std::error::Error>>;

custom_error!{NFO{}  = "Не удалось получить данные."}
custom_error!{WDE{}  = "Не удалось записать данные."}
custom_error!{TNF{}  = "Не удалось найти тег по идентификатору."}

/// Настраивает базу данных.
///
/// Создаёт таблицы, которые будут предназначаться для хранения данных приложения.
pub async fn db_setup(db: &Db) -> MResult<()> {
  db.write_mul(vec![
    ("create table if not exists taskboard_keys (key varchar unique, value varchar);", vec![]),
    ("create table if not exists users (id bigserial, login varchar unique, shared_boards varchar, user_creds varchar, apd varchar);", vec![]),
    ("create table if not exists boards (id bigserial, author bigint, shared_with varchar, header varchar, cards varchar, background varchar);", vec![]),
    ("create table if not exists id_seqs (id varchar unique, val bigint);", vec![])
  ]).await
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
  db.write(
    "insert into users values ($1, $2, '[]', $3, $4);",
    &[&id, &sign_up_credentials.login, &user_credentials, &billing]
  ).await?;
  Ok(id)
}

/// Возвращает идентификатор пользователя по логину и паролю.
pub async fn sign_in_creds_to_id(db: &Db, sign_in_credentials: &SignInCredentials) -> MResult<i64> {
  custom_error!{IncorrectPassword{} = "Неверный пароль!"};
  let id_and_credentials = db.read(
    "select id, user_creds from users where login = $1;", &[&sign_in_credentials.login]
  ).await?;
  let user_credentials: UserCredentials = serde_json::from_str(id_and_credentials.get(1))?;
  match key_gen::check_pass(
    user_credentials.salt,
    user_credentials.salted_pass,
    &sign_in_credentials.pass
  ) {
    true => Ok(id_and_credentials.get(0)),
    _ => Err(Box::new(IncorrectPassword{})),
  }
}

/// Создаёт новый токен и возвращает его.
pub async fn get_new_token(db: &Db, id: &i64) -> MResult<TokenAuth> {
  let user_credentials = db.read("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_credentials: UserCredentials = serde_json::from_str(user_credentials.get(0))?;
  let token = key_gen::generate_strong(64)?;
  let mut hasher = Sha3_256::new();
  hasher.update(&token);
  let hashed = hasher.finalize();
  let token_info = Token {
    tk: hashed.to_vec(),
    from_dt: Utc::now(),
  };
  user_credentials.tokens.push(token_info.clone());
  let user_credentials = serde_json::to_string(&user_credentials)?;
  db.write("update users set user_creds = $1 where id = $2;", &[&user_credentials, id]).await?;
  let token_auth = TokenAuth { id: *id, token };
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
pub async fn write_tokens(db: &Db, id: &i64, tokens: &[Token]) -> MResult<()> {
  let user_credentials = db.read("select user_creds from users where id = $1;", &[id]).await?;
  let mut user_credentials: UserCredentials = serde_json::from_str(user_credentials.get(0))?;
  user_credentials.tokens = tokens.to_owned();
  let user_credentials = serde_json::to_string(&user_credentials)?;
  db.write("update users set user_creds = $1 where id = $2;", &[&user_credentials, id]).await
}

/// Отдаёт список досок пользователя.
pub async fn list_boards(db: &Db, id: &i64) -> MResult<String> {
  let boards = db.read("select shared_boards from users where id = $1;", &[id]).await?;
  let boards: Vec<i64> = serde_json::from_str(boards.get(0))?;
  let mut shorts: Vec<BoardsShort> = vec![];
  for board in &boards {
    let header: String = db.read("select header from boards where id = $1;", &[board]).await?.get(0);
    let header: JsonValue = serde_json::from_str(&header)?;
    let short = BoardsShort {
      id: *board,
      title: header["title"].as_str().unwrap().to_string(),
      header_text_color: header["header_text_color"].as_str().unwrap().to_string(),
      header_background_color: header["header_background_color"].as_str().unwrap().to_string(),
    };
    shorts.push(short);
  }
  let shorts = serde_json::to_string(&shorts)?;
  Ok(shorts)
}

/// Создаёт доску.
pub async fn create_board(db: &Db, author: &i64, board: &Board) -> MResult<i64> {
  custom_error!{EmptyTitle{} = "У доски пустой заголовок."};
  if board.header.title.is_empty() { return Err(Box::new(EmptyTitle{})); };
  if let BoardBackground::Color { color } = &board.background {
    validate_color(color)?;
  };
  validate_color(&board.header.header_background_color)?;
  validate_color(&board.header.header_text_color)?;
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
  let background = serde_json::to_string(&board.background)?;
  let board_queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    (
      "insert into boards values ($1, $2, $3, $4, '[]', $5);",
      vec![&id, author, &shared_with, &header, &background]
    ),
    ("update users set shared_boards = $1 where id = $2;", vec![&shared_boards, author])
  ];
  db.write_mul(board_queries).await?;
  Ok(id)
}

/// Отдаёт доску пользователю.
pub async fn get_board(db: &Db, board_id: &i64) -> MResult<String> {
  let board_data = db.read(
    "select author, shared_with, header, cards, background from boards where id = $1;",
    &[board_id]
  ).await?;
  let author: i64 = board_data.get(0);
  let shared_with: String = board_data.get(1);
  let header: String = board_data.get(2);
  let cards: String = board_data.get(3);
  let background: String = board_data.get(4);
  Ok(
    format!(
      r#"{{"id":{},"author":{},"shared_with":{},"header":{},"cards":{},"background":"{}"}}"#,
      *board_id, author, shared_with, header, cards, background
    )
  )
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
  let mut header_patched: bool = false;
  if let Some(title) = patch.get("title") {
    let title = String::from(title.as_str().ok_or(NFO{})?);
    validate_color(&title)?;
    header.title = title;
    header_patched = true;
  };
  if let Some(background) = patch.get("background") {
    let background_as_struct: BoardBackground = serde_json::from_value(background.clone())?;
    if let BoardBackground::Color { color } = background_as_struct {
      validate_color(&color)?;
    };
    let background = serde_json::to_string(&background)?;
    let r: Vec<&(dyn ToSql + Sync)> = vec![&background, board_id];
    db.write("update boards set background = $1 where id = $2;", &r).await?;
  };
  if let Some(header_background_color) = patch.get("header_background_color") {
    let header_background_color = String::from(header_background_color.as_str().ok_or(NFO{})?);
    validate_color(&header_background_color)?;
    header.header_background_color = header_background_color;
    header_patched = true;
  };
  if let Some(header_text_color) = patch.get("header_text_color") {
    let header_text_color = String::from(header_text_color.as_str().ok_or(NFO{})?);
    validate_color(&header_text_color)?;
    header.header_text_color = header_text_color;
    header_patched = true;
  };
  if header_patched {
    let header = serde_json::to_string(&header)?;
    let r: Vec<&(dyn ToSql + Sync)> = vec![&header, board_id];
    db.write("update boards set header = $1 where id = $2;", &r).await?;
  }
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
  for id_and_shared_board in &ids_and_shared_boards {
    let board_id = *board_id;
    let pair = (id_and_shared_board.0, id_and_shared_board.1.clone());
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
  for result in &results {
    _results.push(result.as_ref().unwrap());
  };
  let results: Vec<&(String, i64)> = _results;
  let mut shared_boards_queries = Vec::new();
  for result in &results {
    let r: Vec<&(dyn ToSql + Sync)> = vec![&result.0, &result.1];
    shared_boards_queries.push(("update users set shared_boards = $1 where id = $2;", r));
  };
  shared_boards_queries.push(("delete from boards where id = $1;", vec![board_id]));
  let board_id_as_str = board_id.to_string();
  shared_boards_queries.push((
    "delete from id_seqs where id like concat($1, '_%');", vec![&board_id_as_str]
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
  match iter.next().ok_or(NFO{})?.iter().any(|id| *id == *board_id) && 
        iter.next().ok_or(NFO{})?.iter().any(|id| *id == *user_id) {
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
  validate_color(&card.background_color)?;
  validate_color(&card.header_text_color)?;
  validate_color(&card.header_background_color)?;
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
    for j in 0..card.tasks[i].tags.len() {
      validate_color(&card.tasks[i].tags[j].background_color)?;
      validate_color(&card.tasks[i].tags[j].text_color)?;
    };
    card.tasks[i].id = next_task_id;
    card.tasks[i].author = *user_id;
    let subtasks_id_seq = tasks_id_seq.clone() + "_" + &next_task_id.to_string();
    next_task_id += 1;
    let mut executors: Vec<i64> = Vec::new();
    card.tasks[i].executors.iter().filter(|e| shared_with.contains(e)).for_each(|i| executors.push(*i));
    card.tasks[i].executors = executors;
    let mut next_subtask_id: i64 = 1;
    for j in 0..card.tasks[i].subtasks.len() {
      for k in 0..card.tasks[i].subtasks[j].tags.len() {
        validate_color(&card.tasks[i].subtasks[j].tags[k].background_color)?;
        validate_color(&card.tasks[i].subtasks[j].tags[k].text_color)?;
      };
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
  for id_seq_query in &id_seqs_queries_data {
    let r: Vec<&(dyn ToSql + Sync)> = vec![&id_seq_query.0, &id_seq_query.1];
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
    let background_color = String::from(background_color.as_str().ok_or(NFO{})?);
    validate_color(&background_color)?;
    card.background_color = background_color;
  };
  if let Some(header_text_color) = patch.get("header_text_color") {
    let header_text_color = String::from(header_text_color.as_str().ok_or(NFO{})?);
    validate_color(&header_text_color)?;
    card.header_text_color = header_text_color;
  };
  if let Some(header_background_color) = patch.get("header_background_color") {
    let header_background_color = String::from(header_background_color.as_str().ok_or(NFO{})?);
    validate_color(&header_background_color)?;
    card.header_background_color = header_background_color;
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
  for i in 0..task.tags.len() {
    validate_color(&task.tags[i].background_color)?;
    validate_color(&task.tags[i].text_color)?;
  };
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
    for j in 0..task.subtasks[i].tags.len() {
      validate_color(&task.subtasks[i].tags[j].background_color)?;
      validate_color(&task.subtasks[i].tags[j].text_color)?;
    };
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
  for i in 0..subtask.tags.len() {
    validate_color(&subtask.tags[i].background_color)?;
    validate_color(&subtask.tags[i].text_color)?;
  };
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

/// Получает теги подзадачи.
pub async fn get_subtask_tags(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
) -> MResult<String> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let tags = &cards.get_subtask(card_id, task_id, subtask_id)?.tags;
  Ok(serde_json::to_string(&tags)?)
}

/// Получает теги задачи.
pub async fn get_task_tags(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
) -> MResult<String> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let tags = &cards.get_task(card_id, task_id)?.tags;
  Ok(serde_json::to_string(&tags)?)
}

/// Создаёт тег у подзадачи.
pub async fn create_tag_at_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  tag: &Tag,
) -> MResult<i64> {
  validate_color(&tag.text_color)?;
  validate_color(&tag.background_color)?;
  let subtask_tags_id_seq = 
    board_id.to_string() + "_" + 
    &card_id.to_string() + "_" + 
    &task_id.to_string() + "_" +
    &subtask_id.to_string() + "t";
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("select cards from boards where id = $1;", vec![board_id]),
    ("select val from id_seqs where id = $1;", vec![&subtask_tags_id_seq]),
  ];
  let results = db.read_mul(queries).await?;
  let mut cards: Vec<Card> = serde_json::from_str(results[0].get(0))?;
  let mut id: i64 = results[1].try_get(0).unwrap_or(0);
  id += 1;
  let mut tag = tag.clone();
  tag.id = id;
  cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags.push(tag);
  let cards = serde_json::to_string(&cards)?;
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
    (
      "insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;",
      vec![&subtask_tags_id_seq, &id],
    ),
  ];
  db.write_mul(queries).await?;
  Ok(id)
}

/// Создаёт тег у задачи.
pub async fn create_tag_at_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  tag: &Tag,
) -> MResult<i64> {
  validate_color(&tag.text_color)?;
  validate_color(&tag.background_color)?;
  let task_tags_id_seq = 
    board_id.to_string() + "_" + 
    &card_id.to_string() + "_" + 
    &task_id.to_string() + "t";
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("select cards from boards where id = $1;", vec![board_id]),
    ("select val from id_seqs where id = $1;", vec![&task_tags_id_seq]),
  ];
  let results = db.read_mul(queries).await?;
  let mut cards: Vec<Card> = serde_json::from_str(results[0].get(0))?;
  let mut id: i64 = results[1].try_get(0).unwrap_or(0);
  id += 1;
  let mut tag = tag.clone();
  tag.id = id;
  cards.get_mut_task(card_id, task_id)?.tags.push(tag);
  let cards = serde_json::to_string(&cards)?;
  let queries: Vec<(&str, Vec<&(dyn ToSql + Sync)>)> = vec![
    ("update boards set cards = $1 where id = $2;", vec![&cards, board_id]),
    (
      "insert into id_seqs values ($1, $2) on conflict (id) do update set val = excluded.val;",
      vec![&task_tags_id_seq, &id],
    ),
  ];
  db.write_mul(queries).await?;
  Ok(id)
}

/// Редактирует тег в подзадаче.
pub async fn patch_tag_at_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  tag_id: &i64,
  patch: &JsonValue,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let mut tags = cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags.clone();
  let mut patched: bool = false;
  for tag in &mut tags {
    if tag.id == *tag_id {
      patched = true;
      if let Some(title) = patch.get("title") {
        tag.title = String::from(title.as_str().ok_or(NFO{})?);
      };
      if let Some(background_color) = patch.get("background_color") {
        let background_color = String::from(background_color.as_str().ok_or(NFO{})?);
        validate_color(&background_color)?;
        tag.background_color = background_color;
      };
      if let Some(text_color) = patch.get("text_color") {
        let text_color = String::from(text_color.as_str().ok_or(NFO{})?);
        validate_color(&text_color)?;
        tag.text_color = text_color;
      };
      break;
    };
  };
  if patched {
    cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags = tags.to_vec();
    let cards = serde_json::to_string(&cards)?;
    db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
  } else {
    Err(Box::new(TNF{}))
  }
}

/// Редактирует тег в задаче.
pub async fn patch_tag_at_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  tag_id: &i64,
  patch: &JsonValue,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let mut tags = cards.get_mut_task(card_id, task_id)?.tags.clone();
  let mut patched: bool = false;
  for tag in &mut tags {
    if tag.id == *tag_id {
      patched = true;
      if let Some(title) = patch.get("title") {
        tag.title = String::from(title.as_str().ok_or(NFO{})?);
      };
      if let Some(background_color) = patch.get("background_color") {
        let background_color = String::from(background_color.as_str().ok_or(NFO{})?);
        validate_color(&background_color)?;
        tag.background_color = background_color;
      };
      if let Some(text_color) = patch.get("text_color") {
        let text_color = String::from(text_color.as_str().ok_or(NFO{})?);
        validate_color(&text_color)?;
        tag.text_color = text_color;
      };
      break;
    };
  };
  if patched {
    cards.get_mut_task(card_id, task_id)?.tags = tags.to_vec();
    let cards = serde_json::to_string(&cards)?;
    db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
  } else {
    Err(Box::new(TNF{}))
  }
}

/// Удаляет тег подзадачи.
pub async fn delete_tag_at_subtask(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  subtask_id: &i64,
  tag_id: &i64,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let mut tags = cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags.clone();
  tags.remove(tags.iter().position(|x| x.id == *tag_id).ok_or(NFO{})?);
  cards.get_mut_subtask(card_id, task_id, subtask_id)?.tags = tags.to_vec();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}

/// Удаляет тег задачи.
pub async fn delete_tag_at_task(
  db: &Db,
  board_id: &i64,
  card_id: &i64,
  task_id: &i64,
  tag_id: &i64,
) -> MResult<()> {
  let cards = db.read("select cards from boards where id = $1;", &[board_id]).await?;
  let mut cards: Vec<Card> = serde_json::from_str(cards.get(0))?;
  let mut tags = cards.get_mut_task(card_id, task_id)?.tags.clone();
  tags.remove(tags.iter().position(|x| x.id == *tag_id).ok_or(NFO{})?);
  cards.get_mut_task(card_id, task_id)?.tags = tags.to_vec();
  let cards = serde_json::to_string(&cards)?;
  db.write("update boards set cards = $1 where id = $2;", &[&cards, board_id]).await
}
