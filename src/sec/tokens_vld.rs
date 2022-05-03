//! Отвечает за токены и оплату аккаунта.

use chrono::{Utc, Duration};

use crate::core::{get_tokens_and_billing, write_tokens};
use crate::psql_handler::Db;
use crate::sec::auth::TokenAuth;

/// 1. Проверяет все токены пользователя на срок годности, проверяет наличие текущего токена и возвращает true, если пользователь определён.
/// 2. Проверяет данные оплаты и возвращает true, если пользователь имеет оплаченный аккаунт.
///
/// TODO сделать Redis-подключение и хранить данные по токенам вместо того, чтобы каждый раз валидировать их через базу данных.
/// WARNING проверка оплаты идёт каждый 31 день, а не ровно в день оплаты
/// TODO Не хранить токены в открытом виде!
pub async fn verify_user(db: &Db, token_auth: &TokenAuth) -> (bool, bool) {
  let (mut tokens, billing) = get_tokens_and_billing(db, &token_auth.id).await.unwrap();
  // 1. Проверка токенов
  let mut s: usize = 0;
  let mut i: usize = 0;
  let mut validated: bool = false;
  while s + i < tokens.len() {
    if s > 0 {
      tokens[i].tk = tokens[i + s].tk.clone();
      tokens[i].from_dt = tokens[i + s].from_dt;
    }
    let duration: Duration = Utc::now() - tokens[i].from_dt;
    if duration.num_days() >= 5 {
      s += 1;
    } else {
      if tokens[i].tk == token_auth.token {
        validated = true;
        tokens[i].from_dt = Utc::now();
      }
      i += 1;
    }
  }
  tokens.truncate(tokens.len() - s);
  // 2. Проверка оплаты
  let mut billed: bool = false;
  if !billing.billed_forever {
    if billing.is_paid_whenever {
      let duration: Duration = Utc::now() - billing.last_payment;
      if duration.num_days() < 31 {
        billed = true;
      } /* else {} */ // Если время истекло, нам нужно узнать у сервера, оплачен ли текущий месяц.
    }
  } else {
    billed = true;
  }
  // X. Возврат результатов
  if (s > 0) || validated {
    match write_tokens(db, &token_auth.id, &tokens).await {
      Err(_) => (false, billed),
      Ok(_) => (validated, billed),
    }
  } else {
    (validated, billed)
  }
}
