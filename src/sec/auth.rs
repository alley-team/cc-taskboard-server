use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use chrono::{DateTime, Utc, serde::ts_seconds};

/// Сведения аутентификации администратора.
#[derive(Deserialize, Serialize)]
pub struct AdminCredentials {
  pub key: String,
}

/// Токен авторизации. Используется при необходимости получить/передать данные.
#[derive(Deserialize, Serialize, Clone)]
pub struct TokenAuth {
  pub id: i64,
  pub token: String,
}

/// Токены аутентификации.
#[derive(Deserialize, Serialize, Clone)]
pub struct Token {
  /// Уникальный идентификатор
  pub tk: String,
  /// Дата и время последнего использования токена.
  /// WARNING Токены действительны не более пяти дней, в течение которых вы ими не пользуетесь.
  #[serde(with = "ts_seconds")]
  pub from_dt: DateTime<Utc>,
}

/// Сведения авторизации пользователя. При входе в аккаунт преобразуются в id и токен (см. ниже).
#[derive(Deserialize, Serialize)]
pub struct SignInCredentials {
  pub login: String,
  pub pass: String,
}

/// Сведения пользователя для регистрации.
#[derive(Deserialize, Serialize)]
pub struct SignUpCredentials {
  pub login: String,
  pub pass: String,
  pub cc_key: String,
}

/// Сведения авторизации пользователя. Используется для хранения данных в БД, так как сохраняет токены.
#[derive(Deserialize, Serialize)]
pub struct UserCredentials {
  pub salt: String,
  pub salted_pass: String,
  pub tokens: Vec<Token>,
}

/// Данные об оплате пользовательского аккаунта.
#[derive(Deserialize, Serialize)]
pub struct AccountPlanDetails {
  /// Некоторые аккаунты оплачиваются навсегда, некоторые - по ежемесячной подписке.
  pub billed_forever: bool,
  /// Данные, которые передаются на внешний API, чтобы узнать состояние подписки.
  pub payment_data: String,
  /// Дата и время совершения последнего платежа (для ежемесячной подписки).
  #[serde(with = "ts_seconds")]
  pub last_payment: DateTime<Utc>,
}

/// Извлекает из запроса ключ администратора.
pub fn extract_creds<T>(header: Option<&hyper::header::HeaderValue>) -> Result<T, ()> 
  where
    T: DeserializeOwned,
{
  let creds = match header {
    None => return Err(()),
    Some(v) => v,
  };
  let creds = match creds.to_str() {
    Err(_) => return Err(()),
    Ok(v) => String::from(v),
  };
  let creds = match base64::decode(&creds) {
    Err(_) => return Err(()),
    Ok(v) => match String::from_utf8(v) {
      Err(_) => return Err(()),
      Ok(v) => v,
    },
  };
  match serde_json::from_str::<T>(&creds) {
    Err(_) => return Err(()),
    Ok(v) => Ok(v),
  }
}
