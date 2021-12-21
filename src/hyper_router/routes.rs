use std::sync::Arc;
use hyper::Body;
use hyper::http::Response;

use crate::hyper_router::resp;
use crate::model::{extract, Board, Workspace};
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

/// Отвечает за регистрацию нового пользователя. 
/// 
/// Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор).
pub async fn sign_up(ws: Workspace) -> Response<Body> {
  let su_creds = match extract_creds::<SignUpCredentials>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let cc_key_id = match psql_handler::check_cc_key(Arc::clone(&ws.cli), su_creds.cc_key.clone()).await {
    Err(_) => return resp::from_code_and_msg(401, Some("Ключ регистрации недействителен.".into())),
    Ok(v) => v,
  };
  if let Err(_) = psql_handler::remove_cc_key(Arc::clone(&ws.cli), cc_key_id).await {
    return resp::from_code_and_msg(401, Some("Ключ регистрации недействителен.".into()));
  };
  let id = match psql_handler::create_user(Arc::clone(&ws.cli), su_creds).await {
    Err(_) => return resp::from_code_and_msg(500, Some("Не удалось создать пользователя.".into())),
    Ok(v) => v,
  };
  match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
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
  let id = match psql_handler::sign_in_creds_to_id(Arc::clone(&ws.cli), si_creds).await {
    Err(_) => return resp::from_code_and_msg(401, None),
    Ok(v) => v,
  };
  if id == -1 {
    return resp::from_code_and_msg(401, None);
  };
  let token_auth = match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
    Err(_) => return resp::from_code_and_msg(500, None),
    Ok(v) => v,
  };
  match serde_json::to_string(&token_auth) {
    Err(_) => resp::from_code_and_msg(500, None),
    Ok(body) => resp::from_code_and_msg(200, Some(body)),
  }
}

/* Все следующие методы обязаны содержать в теле запроса JSON с TokenAuth. */

/// Создаёт пейдж для пользователя.
/// TODO переделать аутентификацию через sec::auth::unwrap_id
pub async fn create_board(ws: Workspace) -> Response<Body> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Err(_) => return resp::from_code_and_msg(401, Some("Не получен валидный токен.".into())),
    Ok(v) => v,
  };
  let (valid, billed) = tokens_vld::verify_user(Arc::clone(&ws.cli), token_auth.clone()).await;
  if !valid {
    return resp::from_code_and_msg(401, Some("Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  let board = match extract::<Board>(ws.req).await {
    Err(_) => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.".into())),
    Ok(v) => v,
  };
  if !billed {
    let boards_n = match psql_handler::count_boards(Arc::clone(&ws.cli), token_auth.id).await {
      Err(_) => return resp::from_code_and_msg(500, Some("Невозможно сосчитать число имеющихся досок у пользователя.".into())),
      Ok(v) => v,
    };
    if boards_n > 0 {
      return resp::from_code_and_msg(402, Some("Вы не можете использовать больше одной доски на бесплатном аккаунте.".into()));
    };
  }
  match psql_handler::create_board(Arc::clone(&ws.cli), token_auth.id, board).await {
    Err(_) => resp::from_code_and_msg(500, Some("База данных сгенерировала ошибку при создании доски.".into())),
    Ok(-1) => resp::from_code_and_msg(400, Some("Данные о новой доске некорректны.".into())),
    Ok(id) => resp::from_code_and_msg(200, Some(id.to_string())),
  }
}
