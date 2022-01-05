//! Отвечает за отдачу методов, в том числе результаты запроса, статус-коды и текст ошибок.

use hyper::Body;
use hyper::http::Response;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::hyper_router::resp;
use crate::model::{extract, Board, Card, Task, Workspace};
use crate::psql_handler;
use crate::sec::auth::{extract_creds, AdminCredentials, TokenAuth, SignInCredentials, SignUpCredentials};
use crate::sec::tokens_vld;

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
pub async fn db_setup(ws: Workspace) -> Response<Body> {
  let admin_key = match extract_creds::<AdminCredentials>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v.key,
  };
  let status_code = match admin_key == ws.cnf.admin_key {
    false => 401,
    true => match psql_handler::db_setup(ws.cli).await {
      Ok(_) => 200,
      Err(_) => 500,
    },
  };
  resp::from_code_and_msg(status_code, None)
}

/// Генерирует новый ключ регистрации по запросу администратора.
pub async fn get_new_cc_key(ws: Workspace) -> Response<Body> {
  let admin_key = match extract_creds::<AdminCredentials>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v.key,
  };
  if admin_key != ws.cnf.admin_key {
    return resp::from_code_and_msg(401, None);
  }
  match psql_handler::register_new_cc_key(ws.cli).await {
    Err(_) => resp::from_code_and_msg(500, None),
    Ok(key) => resp::from_code_and_msg(200, Some(key)),
  }
}

/// Отвечает за регистрацию нового пользователя. 
///
/// Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор).
pub async fn sign_up(ws: Workspace) -> Response<Body> {
  let su_creds = match extract_creds::<SignUpCredentials>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let cc_key_id = match psql_handler::check_cc_key(Arc::clone(&ws.cli), &su_creds.cc_key).await {
    Err(_) => return resp::from_code_and_msg(401, Some("Ключ регистрации не найден.".into())),
    Ok(v) => v,
  };
  if su_creds.pass.len() < 8 {
    return resp::from_code_and_msg(400, Some("Пароль слишком короткий.".into()));
  };
  if let Err(_) = psql_handler::remove_cc_key(Arc::clone(&ws.cli), &cc_key_id).await {
    return resp::from_code_and_msg(401, Some("Ключ регистрации не удалось удалить.".into()));
  };
  let id = match psql_handler::create_user(Arc::clone(&ws.cli), &su_creds).await {
    Err(_) => return resp::from_code_and_msg(500, Some("Не удалось создать пользователя.".into())),
    Ok(v) => v,
  };
  match psql_handler::get_new_token(ws.cli, &id).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось создать токен.".into())),
    Ok(token_auth) => resp::from_code_and_msg(200, Some(serde_json::to_string(&token_auth).unwrap())),
  }
}

/// Отвечает за аутентификацию пользователей в приложении.
pub async fn sign_in(ws: Workspace) -> Response<Body> {
  let si_creds = match extract_creds::<SignInCredentials>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let id = match psql_handler::sign_in_creds_to_id(Arc::clone(&ws.cli), &si_creds).await {
    Err(_) => return resp::from_code_and_msg(401, None),
    Ok(v) => v,
  };
  let token_auth = match psql_handler::get_new_token(ws.cli, &id).await {
    Err(_) => return resp::from_code_and_msg(500, None),
    Ok(v) => v,
  };
  match serde_json::to_string(&token_auth) {
    Err(_) => resp::from_code_and_msg(500, None),
    Ok(body) => resp::from_code_and_msg(200, Some(body)),
  }
}

/// Создаёт доску для пользователя.
pub async fn create_board(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, billed) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let board = match extract::<Board>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  if !billed {
    let boards_n = match psql_handler::count_boards(Arc::clone(&ws.cli), &token_auth.id).await {
      Err(_) => return resp::from_code_and_msg(500, Some("Невозможно сосчитать число имеющихся досок у пользователя.".into())),
      Ok(v) => v,
    };
    if boards_n > 0 {
      return resp::from_code_and_msg(402, Some("Вы не можете использовать больше одной доски на бесплатном аккаунте.".into()));
    };
  }
  match psql_handler::create_board(ws.cli, &token_auth.id, &board).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось создать доску.".into())),
    Ok(id) => resp::from_code_and_msg(200, Some(id.to_string())),
  }
}

/// Передаёт доску пользователю.
pub async fn get_board(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let board_id = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => match v["board_id"].as_i64() {
      None => return resp::from_code_and_msg(400, Some("Не получен board_id.".into())),
      Some(v) => v,
    },
  };
  if let Err(_) = psql_handler::in_shared_with(Arc::clone(&ws.cli), &token_auth.id, &board_id).await {
    return resp::from_code_and_msg(401, Some("Данная доска вам недоступна.".into()));
  };
  let board = match psql_handler::get_board(ws.cli, &board_id).await {
    Err(_) => return resp::from_code_and_msg(500, None),
    Ok(board) => board,
  };
  match serde_json::to_string(&board) {
    Err(_) => resp::from_code_and_msg(500, None),
    Ok(body) => resp::from_code_and_msg(200, Some(body)),
  }
}

/// Патчит доску, изменяя в ней определённые свойства.
///
/// Для доски это - title и background_color. Дочерними карточками управляют методы карточек.
///
/// Запрос представляет из себя JSON с id доски. Изменения принимаются только тогда, когда автором доски является данный пользователь.
pub async fn patch_board(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let patch = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  if patch.get("board_id") == None {
    return resp::from_code_and_msg(400, Some("Не получен board_id.".into()));
  };
  match psql_handler::apply_patch_on_board(ws.cli, &token_auth.id, &patch).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось применить патч к доске.".into())),
    Ok(_) => resp::from_code_and_msg(200, None),
  }
}

/// Удаляет доску.
pub async fn delete_board(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let patch = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  let board_id = match patch.get("board_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен board_id.".into())),
    Some(id) => match id.as_i64() {
      None => return resp::from_code_and_msg(400, None),
      Some(id) => id,
    },
  };
  match psql_handler::remove_board(ws.cli, &token_auth.id, &board_id).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось удалить доску.".into())),
  }
}

/// Создаёт карточку в заданной доске.
pub async fn create_card(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let body = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  let board_id = match body.get("board_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен board_id.".into())),
    Some(id) => match id.as_i64() {
      None => return resp::from_code_and_msg(400, Some("board_id должен быть числом.".into())),
      Some(id) => id,
    },
  };
  if psql_handler::in_shared_with(Arc::clone(&ws.cli), &token_auth.id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Пользователь не имеет доступа к доске.".into()));
  };
  let card: Card = match body.get("card") {
    None => return resp::from_code_and_msg(400, Some("Не получена карточка.".into())),
    Some(card) => match serde_json::from_value(card.clone()) {
      Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать карточку.".into())),
      Ok(card) => card,
    },
  };
  match psql_handler::insert_card(ws.cli, &token_auth.id, &board_id, card).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось добавить карточку.".into())),
    Ok(card_id) => resp::from_code_and_msg(200, Some(card_id.to_string())),
  }
}

/// Патчит карточку, изменяя определённые свойства в ней.
///
/// Для карточки это - title, background_color и text_color.
pub async fn patch_card(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let patch = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  if patch.get("board_id") == None {
    return resp::from_code_and_msg(400, Some("Не получен board_id.".into()));
  };
  if patch.get("card_id") == None {
    return resp::from_code_and_msg(400, Some("Не получен card_id.".into()));
  };
  match psql_handler::apply_patch_on_card(ws.cli, &token_auth.id, &patch).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось применить патч к доске.".into())),
    Ok(_) => resp::from_code_and_msg(200, None),
  }
}

/// Удаляет карточку.
pub async fn delete_card(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let body = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  let board_id = match body.get("board_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен board_id.".into())),
    Some(v) => match v.as_i64() {
      None => return resp::from_code_and_msg(400, Some("board_id должен быть числом.".into())),
      Some(v) => v,
    },
  };
  let card_id = match body.get("card_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен card_id.".into())),
    Some(v) => match v.as_i64() {
      None => return resp::from_code_and_msg(400, Some("card_id должен быть числом.".into())),
      Some(v) => v,
    },
  };
  match psql_handler::remove_card(ws.cli, &token_auth.id, &board_id, &card_id).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось удалить карточку.".into())),
    Ok(_) => resp::from_code_and_msg(200, None),
  }
}

/// Создаёт задачу.
pub async fn create_task(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, _) = tokens_vld::verify_user(Arc::clone(&ws.cli), &token_auth).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let body = match extract::<JsonValue>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  let board_id = match body.get("board_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен board_id.".into())),
    Some(v) => match v.as_i64() {
      None => return resp::from_code_and_msg(400, Some("board_id должен быть числом.".into())),
      Some(v) => v,
    },
  };
  if psql_handler::in_shared_with(Arc::clone(&ws.cli), &token_auth.id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску.".into()));
  };
  let card_id = match body.get("card_id") {
    None => return resp::from_code_and_msg(400, Some("Не получен card_id.".into())),
    Some(v) => match v.as_i64() {
      None => return resp::from_code_and_msg(400, Some("card_id должен быть числом.".into())),
      Some(v) => v,
    },
  };
  let task: Task = match body.get("task") {
    None => return resp::from_code_and_msg(400, Some("Не получена задача.".into())),
    Some(task) => match serde_json::from_value(task.clone()) {
      Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать задачу.".into())),
      Ok(task) => task,
    },
  };
  match psql_handler::insert_task(ws.cli, &token_auth.id, &board_id, &card_id, task).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось добавить задачу.".into())),
    Ok(task_id) => resp::from_code_and_msg(200, Some(task_id.to_string())),
  }
}

/// Патчит задачу.
