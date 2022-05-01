//! Отвечает за отдачу методов, в том числе результаты запроса, статус-коды и текст ошибок.
//!
//! У всех методов должны проверяться права человека на доску путём просмотра списка shared_with:
//!
//! ```rust
//! if psql_handler::in_shared_with(&ws.db, &token_auth.id, &board_id).await.is_err() {
//!   return resp::from_code_and_msg(500, Some("Пользователь не имеет доступа к доске."));
//! };
//! ```
//!
//! Следствие этого правила: те, кто имеют доступ к доске, могут редактировать всё её содержимое, кроме параметров самой доски.
//!
//! Роутер, в отличие от логики базы данных, отвечает за проверку наличия необходимых параметров в теле запросов. Поэтому все обязательные значения, включая структуры, должны десериализовываться в данном модуле, чтобы в случае чего оперативно предоставить в ответе сервера конкретную ошибку.

use hyper::Body;
use hyper::http::Response;
use serde_json::Value as JsonValue;

use crate::hyper_router::resp;
use crate::model::{extract, Board, Card, Task, Subtask, Tag, Timelines, Workspace};
use crate::psql_handler;
use crate::sec::auth::{extract_creds, AdminCredentials, TokenAuth, SignInCredentials, SignUpCredentials};
use crate::sec::tokens_vld;

/// Отвечает на предзапросы браузера.
pub async fn pre_request() -> Response<Body> {
  resp::options_answer()
}

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
pub async fn db_setup(ws: Workspace, admin_key: String) -> Response<Body> {
  let key = match extract_creds::<AdminCredentials>(ws.req.headers().get("App-Token")) {
    Ok(v) => v.key,
    _ => return resp::from_code_and_msg(401, Some("Не получен валидный токен.")),
  };
  let status_code = match key == admin_key {
    true => match psql_handler::db_setup(&ws.db).await {
      Ok(_) => 200,
      _ => 500,
    },
    _ => 401,
  };
  resp::from_code_and_msg(status_code, None)
}

/// Генерирует новый ключ регистрации по запросу администратора.
pub async fn get_new_cc_key(ws: Workspace, admin_key: String) -> Response<Body> {
  let key = match extract_creds::<AdminCredentials>(ws.req.headers().get("App-Token")) {
    Ok(v) => v.key,
    _ => return resp::from_code_and_msg(401, Some("Не получен валидный токен.")),
  };
  if key != admin_key {
    return resp::from_code_and_msg(401, None);
  }
  match psql_handler::register_new_cc_key(&ws.db).await {
    Ok(key) => resp::from_code_and_msg(200, Some(&key)),
    _ => resp::from_code_and_msg(500, None),
  }
}

/// Отвечает за регистрацию нового пользователя. 
///
/// Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор).
pub async fn sign_up(ws: Workspace) -> Response<Body> {
  let su_creds = match extract_creds::<SignUpCredentials>(ws.req.headers().get("App-Token")) {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(401, Some("Не получен валидный токен.")),
  };
  let cc_key_id = match psql_handler::check_cc_key(&ws.db, &su_creds.cc_key).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(401, Some("Ключ регистрации не найден.")),
  };
  if su_creds.pass.len() < 8 {
    return resp::from_code_and_msg(400, Some("Пароль слишком короткий."));
  };
  if let Err(_) = psql_handler::remove_cc_key(&ws.db, &cc_key_id).await {
    return resp::from_code_and_msg(401, Some("Ключ регистрации не удалось удалить."));
  };
  let id = match psql_handler::create_user(&ws.db, &su_creds).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(500, Some("Не удалось создать пользователя.")),
  };
  match psql_handler::get_new_token(&ws.db, &id).await {
    Ok(token_auth) => resp::from_code_and_msg(200, Some(&serde_json::to_string(&token_auth).unwrap())),
    _ => resp::from_code_and_msg(500, Some("Не удалось создать токен.")),
  }
}

/// Отвечает за аутентификацию пользователей в приложении.
pub async fn sign_in(ws: Workspace) -> Response<Body> {
  let si_creds = match extract_creds::<SignInCredentials>(ws.req.headers().get("App-Token")) {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(401, Some("Не получен валидный токен.")),
  };
  let id = match psql_handler::sign_in_creds_to_id(&ws.db, &si_creds).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(401, None),
  };
  let token_auth = match psql_handler::get_new_token(&ws.db, &id).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(500, None),
  };
  match serde_json::to_string(&token_auth) {
    Ok(body) => resp::from_code_and_msg(200, Some(&body)),
    _ => resp::from_code_and_msg(500, None),
  }
}

/// Аутенцифицирует пользователя по токену, возвращая его идентификатор и данные по оплате аккаунта.
pub async fn auth_by_token(ws: &Workspace) -> Result<(i64, bool), (u16, String)> {
  let token_auth = match extract_creds::<TokenAuth>(ws.req.headers().get("App-Token")) {
    Ok(v) => v,
    _ => return Err((401, "Не получен валидный токен.".into())),
  };
  let (valid, billed) = tokens_vld::verify_user(&ws.db, &token_auth).await;
  if !valid {
    return Err((401, "Неверный токен. Пройдите аутентификацию заново.".into()));
  };
  Ok((token_auth.id, billed))
}

/// Отправляет список доступных для пользователя досок.
pub async fn list_boards(ws: Workspace, user_id: i64) -> Response<Body> {
  match psql_handler::list_boards(&ws.db, &user_id).await {
    Ok(list) => resp::from_code_and_msg(200, Some(&list)),
    _ => resp::from_code_and_msg(500, Some("Не удалось получить список досок.")),
  }
}

/// Создаёт доску для пользователя.
pub async fn create_board(ws: Workspace, user_id: i64, billed: bool) -> Response<Body> {
  if !billed {
    let boards_n = match psql_handler::count_boards(&ws.db, &user_id).await {
      Ok(v) => v,
      _ => return resp::from_code_and_msg(500, Some("Невозможно сосчитать число имеющихся досок у пользователя.")),
    };
    if boards_n > 0 {
      return resp::from_code_and_msg(402, Some("Вы не можете использовать больше одной доски на бесплатном аккаунте."));
    };
  };
  let board = match extract::<Board>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  match psql_handler::create_board(&ws.db, &user_id, &board).await {
    Ok(id) => resp::from_code_and_msg(200, Some(&id.to_string())),
    _ => resp::from_code_and_msg(500, Some("Не удалось создать доску.")),
  }
}

/// Передаёт доску пользователю.
pub async fn get_board(ws: Workspace, user_id: i64) -> Response<Body> {
  let board_id = match extract::<JsonValue>(ws.req).await {
    Ok(v) => match v["board_id"].as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  if let Err(_) = psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await {
    return resp::from_code_and_msg(401, Some("Данная доска вам недоступна."));
  };
  match psql_handler::get_board(&ws.db, &board_id).await {
    Ok(board) => resp::from_code_and_msg(200, Some(&board)),
     _ => resp::from_code_and_msg(500, None),
  }
}

/// Патчит доску, изменяя в ней определённые свойства.
///
/// Для доски это - title, background_color, header_background_color и header_text_color. Дочерними карточками управляют методы карточек.
///
/// Запрос представляет из себя JSON с id доски. Изменения принимаются только тогда, когда автором доски является данный пользователь.
pub async fn patch_board(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(id) => match id.as_i64() {
      Some(id) => id,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  match psql_handler::apply_patch_on_board(&ws.db, &user_id, &board_id, &patch).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось применить патч к доске.")),
  }
}

/// Удаляет доску.
pub async fn delete_board(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(id) => match id.as_i64() {
      Some(id) => id,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  match psql_handler::remove_board(&ws.db, &user_id, &board_id).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось удалить доску.")),
  }
}

/// Создаёт карточку в заданной доске.
pub async fn create_card(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(id) => match id.as_i64() {
      Some(id) => id,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Пользователь не имеет доступа к доске."));
  };
  let card: Card = match body.get("card") {
    Some(card) => match serde_json::from_value(card.clone()) {
      Ok(card) => card,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать карточку.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получена карточка.")),
  };
  match psql_handler::insert_card(&ws.db, &user_id, &board_id, card).await {
    Ok(card_id) => resp::from_code_and_msg(200, Some(&card_id.to_string())),
    _ => resp::from_code_and_msg(500, Some("Не удалось добавить карточку.")),
  }
}

/// Патчит карточку, изменяя определённые свойства в ней.
///
/// Для карточки это - title, background_color, header_background_color и header_text_color.
pub async fn patch_card(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match patch.get("card_id") {
    Some(id) => match id.as_i64() {
      Some(id) => id,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  match psql_handler::apply_patch_on_card(&ws.db, &board_id, &card_id, &patch).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось применить патч к доске.")),
  }
}

/// Удаляет карточку.
pub async fn delete_card(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  match psql_handler::remove_card(&ws.db, &board_id, &card_id).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось удалить карточку.")),
    _ => resp::from_code_and_msg(200, None),
  }
}

/// Создаёт задачу.
pub async fn create_task(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task: Task = match body.get("task") {
    Some(task) => match serde_json::from_value(task.clone()) {
      Ok(task) => task,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать задачу.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получена задача.")),
  };
  match psql_handler::insert_task(&ws.db, &user_id, &board_id, &card_id, task).await {
    Ok(task_id) => resp::from_code_and_msg(200, Some(&task_id.to_string())),
    _ => resp::from_code_and_msg(500, Some("Не удалось добавить задачу.")),
  }
}

/// Патчит задачу.
///
/// В задаче можно поменять:
/// 1. Название задачи.
/// 2. Назначенных исполнителей задачи.
/// 3. Статус выполнения задачи (выполнена/не выполнена).
/// 4. Заметки к задаче.
pub async fn patch_task(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match patch.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match patch.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  match psql_handler::apply_patch_on_task(&ws.db, &board_id, &card_id, &task_id, &patch).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось применить патч к задаче.")),
  }
}

/// Удаляет задачу.
pub async fn delete_task(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  match psql_handler::remove_task(&ws.db, &board_id, &card_id, &task_id).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось удалить задачу.")),
    _ => resp::from_code_and_msg(200, None),
  }
}

/// Изменяет метки задачи.
// pub async fn patch_task_tags(ws: Workspace, user_id: i64) -> Response<Body> {
//   let body = match extract::<JsonValue>(ws.req).await {
//     Ok(v) => v,
//     _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
//   };
//   let board_id = match body.get("board_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
//   };
//   if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
//     return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
//   };
//   let card_id = match body.get("card_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
//   };
//   let task_id = match body.get("task_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
//   };
//   let tags: Vec<Tag> = match body.get("tags") {
//     Some(tags) => match serde_json::from_value(tags.clone()) {
//       Ok(tags) => tags,
//       _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать метки.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получены метки.")),
//   };
//   match psql_handler::set_tags_on_task(&ws.db, &board_id, &card_id, &task_id, &tags).await {
//     Ok(_) => resp::from_code_and_msg(200, None),
//     _ => resp::from_code_and_msg(500, Some("Не удалось присвоить метки для задачи.")),
//   }
// }

/// Изменяет временные рамки задачи.
pub async fn patch_task_time(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let timelines: Timelines = match body.get("timelines") {
    Some(timelines) => match serde_json::from_value(timelines.clone()) {
      Ok(timelines) => timelines,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать временные рамки.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получены временные рамки.")),
  };
  match psql_handler::set_timelines_on_task(&ws.db, &board_id, &card_id, &task_id, &timelines).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось присвоить временные рамки для задачи.")),
  }
}

/// Создаёт подзадачу.
pub async fn create_subtask(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let subtask: Subtask = match body.get("subtask") {
    Some(subtask) => match serde_json::from_value(subtask.clone()) {
      Ok(subtask) => subtask,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать подзадачу.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получена подзадача.")),
  };
  match psql_handler::insert_subtask(&ws.db, &user_id, &board_id, &card_id, &task_id, subtask).await {
    Ok(subtask_id) => resp::from_code_and_msg(200, Some(&subtask_id.to_string())),
    _ => resp::from_code_and_msg(500, Some("Не удалось добавить подзадачу.")),
  }
}

/// Изменяет подзадачу.
///
/// В подзадаче можно поменять:
/// 1. Название подзадачи.
/// 2. Назначенных исполнителей подзадачи.
/// 3. Статус выполнения подзадачи (выполнена/не выполнена).
pub async fn patch_subtask(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match patch.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match patch.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let subtask_id = match patch.get("subtask_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен subtask_id.")),
  };
  match psql_handler::apply_patch_on_subtask(
    &ws.db, &board_id, &card_id, &task_id, &subtask_id, &patch
  ).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось применить патч к подзадаче.")),
  }
}

/// Удаляет подзадачу.
pub async fn delete_subtask(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let subtask_id = match body.get("subtask_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен subtask_id.")),
  };
  match psql_handler::remove_subtask(&ws.db, &board_id, &card_id, &task_id, &subtask_id).await {
    Err(_) => resp::from_code_and_msg(500, Some("Не удалось удалить подзадачу.")),
    _ => resp::from_code_and_msg(200, None),
  }
}

/// Изменяет метки подзадачи.
// pub async fn patch_subtask_tags(ws: Workspace, user_id: i64) -> Response<Body> {
//   let body = match extract::<JsonValue>(ws.req).await {
//     Ok(v) => v,
//     _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
//   };
//   let board_id = match body.get("board_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
//   };
//   if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
//     return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
//   };
//   let card_id = match body.get("card_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
//   };
//   let task_id = match body.get("task_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
//   };
//   let subtask_id = match body.get("subtask_id") {
//     Some(v) => match v.as_i64() {
//       Some(v) => v,
//       _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получен subtask_id.")),
//   };
//   let tags: Vec<Tag> = match body.get("tags") {
//     Some(tags) => match serde_json::from_value(tags.clone()) {
//       Ok(tags) => tags,
//       _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать метки.")),
//     },
//     _ => return resp::from_code_and_msg(400, Some("Не получены метки.")),
//   };
//   match psql_handler::set_tags_on_subtask(
//     &ws.db, &board_id, &card_id, &task_id, &subtask_id, &tags
//   ).await {
//     Ok(_) => resp::from_code_and_msg(200, None),
//     _ => resp::from_code_and_msg(500, Some("Не удалось присвоить метки для подзадачи.")),
//   }
// }

/// Изменяет временные рамки подзадачи.
pub async fn patch_subtask_time(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let subtask_id = match body.get("subtask_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен subtask_id.")),
  };
  let timelines: Timelines = match body.get("timelines") {
    Some(timelines) => match serde_json::from_value(timelines.clone()) {
      Ok(timelines) => timelines,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать временные рамки.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получены временные рамки.")),
  };
  match psql_handler::set_timelines_on_subtask(
    &ws.db, &board_id, &card_id, &task_id, &subtask_id, &timelines
  ).await {
    Ok(_) => resp::from_code_and_msg(200, None),
    _ => resp::from_code_and_msg(500, Some("Не удалось присвоить временные рамки для подзадачи.")),
  }
}

/// Получает теги задачи/подзадачи.
pub async fn get_tags(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  match body.get("subtask_id") {
    Some(subtask_id) => match subtask_id.as_i64() {
      Some(subtask_id) => match psql_handler::get_subtask_tags(
        &ws.db, &board_id, &card_id, &task_id, &subtask_id
      ).await {
        Ok(tags) => resp::from_code_and_msg(200, Some(&tags)),
        _ => resp::from_code_and_msg(500, Some("Не удалось получить теги подзадачи.")),
      },
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => match psql_handler::get_task_tags(
      &ws.db, &board_id, &card_id, &task_id
    ).await {
      Ok(tags) => resp::from_code_and_msg(200, Some(&tags)),
      _ => resp::from_code_and_msg(500, Some("Не удалось получить теги задачи.")),
    },
  }
}

/// Создаёт тег в задаче/подзадаче.
pub async fn create_tag(ws: Workspace, user_id: i64) -> Response<Body> {
  let body = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match body.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match body.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match body.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let tag: Tag = match body.get("tag") {
    Some(tag) => match serde_json::from_value(tag.clone()) {
      Ok(tag) => tag,
      _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать тег.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен тег.")),
  };
  match body.get("subtask_id") {
    Some(subtask_id) => match subtask_id.as_i64() {
      Some(subtask_id) => match psql_handler::create_tag_at_subtask(
        &ws.db, &board_id, &card_id, &task_id, &subtask_id, &tag
      ).await {
        Ok(id) => resp::from_code_and_msg(200, Some(&id.to_string())),
        _ => resp::from_code_and_msg(500, Some("Не удалось прикрепить тег к подзадаче.")),
      },
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => match psql_handler::create_tag_at_task(
      &ws.db, &board_id, &card_id, &task_id, &tag
    ).await {
      Ok(id) => resp::from_code_and_msg(200, Some(&id.to_string())),
      _ => resp::from_code_and_msg(500, Some("Не удалось прикрепить тег к задаче.")),
    },
  }
}

/// Редактирует тег в задаче/подзадаче.
pub async fn patch_tag(ws: Workspace, user_id: i64) -> Response<Body> {
  let patch = match extract::<JsonValue>(ws.req).await {
    Ok(v) => v,
    _ => return resp::from_code_and_msg(400, Some("Не удалось десериализовать данные.")),
  };
  let board_id = match patch.get("board_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("board_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен board_id.")),
  };
  if psql_handler::in_shared_with(&ws.db, &user_id, &board_id).await.is_err() {
    return resp::from_code_and_msg(500, Some("Не удалось проверить права пользователя на доску."));
  };
  let card_id = match patch.get("card_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("card_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен card_id.")),
  };
  let task_id = match patch.get("task_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("task_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен task_id.")),
  };
  let tag_id = match patch.get("tag_id") {
    Some(v) => match v.as_i64() {
      Some(v) => v,
      _ => return resp::from_code_and_msg(400, Some("tag_id должен быть числом.")),
    },
    _ => return resp::from_code_and_msg(400, Some("Не получен tag_id.")),
  };
  match patch.get("subtask_id") {
    Some(subtask_id) => match subtask_id.as_i64() {
      Some(subtask_id) => match psql_handler::patch_tag_at_subtask(
        &ws.db, &board_id, &card_id, &task_id, &subtask_id, &tag_id, &patch
      ).await {
        Ok(id) => resp::from_code_and_msg(200, None),
        _ => resp::from_code_and_msg(500, Some("Не удалось изменить тег.")),
      },
      _ => return resp::from_code_and_msg(400, Some("subtask_id должен быть числом.")),
    },
    _ => match psql_handler::patch_tag_at_task(
      &ws.db, &board_id, &card_id, &task_id, &tag_id, &patch
    ).await {
      Ok(id) => resp::from_code_and_msg(200, None),
      _ => resp::from_code_and_msg(500, Some("Не удалось изменить тег.")),
    },
  }
}

/// Изменяет данные аутентификации пользователя.
pub async fn patch_user_creds(_ws: Workspace, _user_id: i64) -> Response<Body> {
  unimplemented!();
}

/// Изменяет способы оплаты аккаунта пользователя.
pub async fn patch_user_billing(_ws: Workspace, _user_id: i64) -> Response<Body> {
  unimplemented!();
}
