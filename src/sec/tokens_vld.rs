use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;

use chrono::{Utc, Duration};

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

use crate::psql_handler::{get_all_tokens, write_all_tokens, get_new_token};

/// Проверяет все токены пользователя на срок годности, проверяет наличие текущего токена и возвращает true, если пользователь определён.
pub async fn verify_token(cli: PgClient, token_auth: TokenAuth) -> bool {
  let mut tokens = get_all_tokens(Arc::clone(&cli), token_auth.id);
  let s: usize = 0, i: usize = 0;
  let validated: bool = false;
  while s + i < tokens.len() {
    if s > 0 {
      tokens[i] = tokens[i + s];
    }
    let duration = Utc::now() - tokens[i].from_dt;
    let duration = Duration::seconds(duration.timestamp());
    if duration.num_days() >= 5 {
      s += 1;
    } else {
      if tokens[i].tk == token_auth.tk {
        validated = true;
      }
      i += 1;
    }
  }
  tokens.truncate(tokens.len() - s);
  write_all_tokens(Arc::clone(&cli), id, &tokens);
  validated
}
