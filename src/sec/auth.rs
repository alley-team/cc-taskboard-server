use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, serde::ts_seconds};

/// Сведения аутентификации администратора.
#[derive(Deserialize, Serialize)]
pub struct AdminAuth {
  pub key: String,
}

/// Сведения авторизации пользователя. При входе в аккаунт преобразуются в id и токен (см. ниже).
#[derive(Deserialize, Serialize)]
pub struct UserAuth {
  pub login: String,
  pub pass: String,
}

/// Токен авторизации. Используется при необходимости получить/передать данные.
#[derive(Deserialize, Serialize)]
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

/// Сведения пользователя для регистрации.
#[derive(Deserialize, Serialize)]
pub struct RegisterUserData {
  pub login: String,
  pub pass: String,
  pub cc_key: String,
}

/// Сведения авторизации пользователя. Используется для хранения данных в БД, так как сохраняет токены.
#[derive(Deserialize, Serialize)]
pub struct UserAuthData {
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

pub fn parse_admin_auth_key(bytes: hyper::body::Bytes) -> serde_json::Result<String> {
  Ok(serde_json::from_str::<AdminAuth>(&String::from_utf8(bytes.to_vec()).unwrap())?.key)
}
