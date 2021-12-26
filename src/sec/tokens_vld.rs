use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::{Utc, Duration};

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

use crate::psql_handler::{get_tokens_and_billing, write_tokens};
use crate::sec::auth::TokenAuth;

/// 1. Проверяет все токены пользователя на срок годности, проверяет наличие текущего токена и возвращает true, если пользователь определён.
/// 2. Проверяет данные оплаты и возвращает true, если пользователь имеет оплаченный аккаунт.
/// 
/// TODO сделать Redis-подключение и хранить данные по токенам вместо того, чтобы каждый раз валидировать их через базу данных.
/// WARNING проверка оплаты идёт каждый 31 день, а не ровно в день оплаты
pub async fn verify_user(cli: PgClient, token_auth: &TokenAuth) -> (bool, bool) {
  let (mut tokens, billing) = get_tokens_and_billing(Arc::clone(&cli), &token_auth.id).await.unwrap();
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
    match write_tokens(Arc::clone(&cli), &token_auth.id, &tokens).await {
      Err(_) => (false, billed),
      Ok(_) => (validated, billed),
    }
  } else {
    (validated, billed)
  }
}
