use std::sync::Arc;
use hyper::Body;
use hyper::body::to_bytes as body_to_bytes;
use hyper::http::{Response, Result as HttpResult};

use crate::hyper_router::resp;
use crate::model::Workspace;
use crate::psql_handler;
use crate::sec::auth::{host_key, TokenAuth, SignInCredentials, SignUpCredentials};
use crate::sec::tokens_vld;

/// Отвечает за авторизацию администратора и первоначальную настройку базы данных.
pub async fn db_setup(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(Response::builder()
    .status(match host_key(body_to_bytes(ws.req.into_body()).await.unwrap()).unwrap() == ws.cnf.admin_key {
      false => 401,
      true => match psql_handler::db_setup(ws.cli).await {
        Ok(_) => 200,
        Err(_) => 500,
      }
    })
    .body(Body::empty())?)
}

/// Отвечает за регистрацию нового пользователя. 
/// 
/// Создаёт аккаунт и возвращает данные аутентификации (новый токен и идентификатор).
pub async fn sign_up(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<SignUpCredentials>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => resp::from_code_and_msg(400, Some(String::from("Переданные данные повреждены."))),
    Ok(su_creds) => match psql_handler::check_cc_key(Arc::clone(&ws.cli),
                                                          su_creds.cc_key.clone()).await {
      Err(_) => resp::from_code_and_msg(401, Some(String::from("Ключ регистрации недействителен."))),
      Ok(key_id) => {
        if let Err(_) = psql_handler::remove_cc_key(Arc::clone(&ws.cli), key_id).await {
          return Ok(resp::from_code_and_msg(401, Some(String::from("Ключ регистрации недействителен."))));
        };
        match psql_handler::create_user(Arc::clone(&ws.cli), su_creds).await {
          Err(_) => resp::from_code_and_msg(500, Some(String::from("Не удалось создать пользователя."))),
          Ok(id) => match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
            Err(_) => resp::from_code_and_msg(500, Some(String::from("Не удалось создать токен."))),
            Ok(token_auth) => resp::from_code_and_msg(200, Some(serde_json::to_string(&token_auth).unwrap())),
          },
        }
      }
    },
  })
}

/// Отвечает за аутентификацию пользователей в приложении.
pub async fn sign_in(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<SignInCredentials>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => resp::from_code_and_msg(400, None),
    Ok(si_creds) => match psql_handler::sign_in_creds_to_id(Arc::clone(&ws.cli), si_creds).await {
      Err(_) => resp::from_code_and_msg(401, None),
      Ok(id) => {
        if id == -1 {
          resp::from_code_and_msg(401, None)
        } else { 
          match psql_handler::get_new_token(Arc::clone(&ws.cli), id).await {
            Err(_) => resp::from_code_and_msg(500, None),
            Ok(token_auth) => resp::from_code_and_msg(200, Some(serde_json::to_string(&token_auth).unwrap())),
          }
        }
      },
    },
  })
}

// Все следующие методы обязаны содержать в теле запроса JSON с TokenAuth.

/// Создаёт пейдж для пользователя.
/// TODO переделать аутентификацию через sec::auth::unwrap_id
pub async fn create_board(ws: Workspace) -> HttpResult<Response<Body>> {
  Ok(match serde_json::from_str::<serde_json::Value>(&String::from_utf8(
    body_to_bytes(ws.req.into_body()).await.unwrap().to_vec()).unwrap())
  {
    Err(_) => resp::from_code_and_msg(400, None),
    Ok(task) => {
      let token_auth: serde_json::Result<TokenAuth> = serde_json::from_str(&serde_json::to_string(&task["token_auth"]).unwrap());
      match token_auth {
        Err(_) => resp::from_code_and_msg(400, None),
        Ok(token_auth) => match tokens_vld::verify_user(Arc::clone(&ws.cli), token_auth.clone()).await {
          (false, _) => resp::from_code_and_msg(401, None),
          (true, billing) => match psql_handler::count_boards(Arc::clone(&ws.cli), token_auth.id).await {
            Err(_) => resp::from_code_and_msg(500, Some(String::from("Невозможно сосчитать число имеющихся досок у пользователя."))),
            Ok(boards_num) => match (boards_num > 0) ^ billing {
              true => resp::from_code_and_msg(402, None),
              false => {
                let title = task["title"].as_str().unwrap().to_string(); 
                let background_color = task["background_color"].as_str().unwrap().to_string();
                match psql_handler::create_board(Arc::clone(&ws.cli), token_auth.id, title, background_color).await {
                  Err(_) => resp::from_code_and_msg(500, None),
                  Ok(-1) => resp::from_code_and_msg(400, None),
                  Ok(id) => resp::from_code_and_msg(200, Some(id.to_string())),
                }
              },
            },
          },
        },
      }
    },
  })
}
