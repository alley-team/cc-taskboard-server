//! Предоставляет структуры данных для управления аутентификацией.

use chrono::{DateTime, Utc, serde::ts_seconds};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// Сведения аутентификации администратора.
#[derive(Deserialize, Serialize)]
pub struct AdminCredentials {
  /// Ключ администратора.
  pub key: String,
}

/// Токен аутентификации. Используется при необходимости получить/передать данные.
#[derive(Deserialize, Serialize, Clone)]
pub struct TokenAuth {
  /// Идентификатор пользователя.
  pub id: i64,
  /// Токен.
  pub token: String,
}

/// Представление токена аутентификации в базе данных.
#[derive(Deserialize, Serialize, Clone)]
pub struct Token {
  /// Хэш токена SHA3-256.
  pub tk: Vec<u8>,
  /// Дата и время последнего использования токена.
  ///
  /// Токены действительны не более пяти дней, в течение которых вы ими не пользуетесь.
  #[serde(with = "ts_seconds")]
  pub from_dt: DateTime<Utc>,
}

/// Сведения авторизации пользователя. При входе в аккаунт преобразуются в id и токен (см. ниже).
#[derive(Deserialize, Serialize)]
pub struct SignInCredentials {
  /// Логин.
  pub login: String,
  /// Пароль.
  pub pass: String,
}

/// Сведения пользователя для регистрации.
#[derive(Deserialize, Serialize)]
pub struct SignUpCredentials {
  /// Логин.
  ///
  /// Должен быть уникальным для успешной регистрации. Может содержать любые спецсимволы, пробелы, в том числе в начале/конце.
  pub login: String,
  /// Пароль.
  ///
  /// Должен быть не менее 8 символов в длину, если передаётся в чистом виде; или может быть представлен в виде хэша парольной строки, также преобразованный в строку.
  pub pass: String,
}

/// Сведения авторизации пользователя. Используется для хранения данных в БД, так как сохраняет токены.
///
/// Для недопущения компрометации паролей пользователей в базе данных хранятся не они сами - и даже не их хэши! - а две компоненты: соль и подсоленный пароль. Аутентификация проходит следующим образом: пароль, полученный от клиента, подсаливается и сравнивается с подсоленным паролем из базы данных.
#[derive(Deserialize, Serialize)]
pub struct UserCredentials {
  /// Соль.
  pub salt: Vec<u8>,
  /// Подсоленный пароль.
  pub salted_pass: Vec<u8>,
  /// Список токенов.
  pub tokens: Vec<Token>,
}

/// Данные об оплате пользовательского аккаунта.
#[derive(Deserialize, Serialize)]
pub struct AccountPlanDetails {
  /// Некоторые аккаунты оплачиваются навсегда, некоторые - по ежемесячной подписке.
  pub billed_forever: bool,
  /// Данные, которые передаются на внешний API, чтобы узнать состояние подписки.
  pub payment_data: String,
  /// Указывает на то, стоит ли доверять нижеуказанным данным.
  pub is_paid_whenever: bool,
  /// Дата и время совершения последнего платежа (для ежемесячной подписки).
  #[serde(with = "ts_seconds")]
  pub last_payment: DateTime<Utc>,
}

/// Парсит заголовок App-Token HTTP-запроса в необходимую структуру.
///
/// Данные в заголовке передаются в base64-кодировке и представляют из себя JSON-структуру.
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
