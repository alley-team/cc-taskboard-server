use std::sync::Arc;
use tokio::sync::Mutex;
use chrono::Utc;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

use crate::psql_handler::{get_all_tokens, write_all_tokens};
use crate::sec::auth::TokenAuth;

/// Проверяет все токены пользователя на срок годности, проверяет наличие текущего токена и возвращает true, если пользователь определён.
pub async fn verify_token(cli: PgClient, token_auth: TokenAuth) -> bool {
  let mut tokens = get_all_tokens(Arc::clone(&cli), token_auth.id).await.unwrap();
  let mut s: usize = 0;
  let mut i: usize = 0;
  let mut validated: bool = false;
  while s + i < tokens.len() {
    if s > 0 {
      tokens[i].tk = tokens[i + s].tk.clone();
      tokens[i].from_dt = tokens[i + s].from_dt;
    }
    let duration = Utc::now() - tokens[i].from_dt;
    if duration.num_days() >= 5 {
      s += 1;
    } else {
      if tokens[i].tk == token_auth.token {
        validated = true;
      }
      i += 1;
    }
  }
  tokens.truncate(tokens.len() - s);
  match write_all_tokens(Arc::clone(&cli), token_auth.id, tokens).await {
    Err(_) => false,
    Ok(_) => validated,
  }
}
