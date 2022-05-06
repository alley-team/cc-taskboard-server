//! Отвечает за переход от старых форматов данных к новым.

use chrono::{DateTime, Utc, serde::ts_seconds};
use serde::{Deserialize, Serialize};

use crate::model::BoardBackground;
use crate::psql_handler::Db;
use crate::sec::auth::UserCredentials;

type MResult<T> = Result<T, Box<dyn std::error::Error>>;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ########################################################################################
//
// ОБНОВЛЕНИЕ 2.3.2->2.3.3
//
// 1. Токен заменён на хэш, его репрезентация: String -> Vec<u8>
// 2. Фоновый цвет доски заменён на более общую структуру данных: String -> BoardBackground
//
// ########################################################################################

/// Представление токена аутентификации версии 2.3.2 и ниже.
#[derive(Deserialize, Serialize, Clone)]
pub struct Token2_3_2 {
  /// Токен.
  pub tk: String,
  /// Дата и время последнего использования токена.
  ///
  /// Токены действительны не более пяти дней, в течение которых вы ими не пользуетесь.
  #[serde(with = "ts_seconds")]
  pub from_dt: DateTime<Utc>,
}

/// Версия пользовательских данных версии 2.3.2 и ниже.
#[derive(Deserialize, Serialize)]
pub struct UserCredentials2_3_2 {
  /// Соль.
  pub salt: Vec<u8>,
  /// Подсоленный пароль.
  pub salted_pass: Vec<u8>,
  /// Список токенов.
  pub tokens: Vec<Token2_3_2>,
}



/// Обновляет репрезентацию фона в досках из версии 2.3.2 и ниже.
pub fn integrate_boards_background_232_to_cur(background_color: &str) -> BoardBackground {
  BoardBackground::Color { color: background_color.to_owned() }
}

/// Обновляет репрезентацию данных пользователя из версии 2.3.2 и ниже.
///
/// Из-за обновления безопасности все старые токены удаляются.
pub fn integrate_user_creds_232_to_cur(user_credentials: &str) -> MResult<UserCredentials> {
  let user_creds: UserCredentials2_3_2 = serde_json::from_str(user_credentials)?;
  Ok(UserCredentials {
    salt: user_creds.salt.clone(),
    salted_pass: user_creds.salted_pass.clone(),
    tokens: vec![],
  })
}

// Общие функции.

/// Возвращает версию базы данных.
pub async fn check_tbs_db_ver(db: &Db) -> String {
  let keys_table_existence = db.read(
    "select exists (select from pg_tables where schemaname = 'public' and tablename = 'taskboard_keys');",
    &[]
  ).await.unwrap();
  let keys_table_existence: bool = keys_table_existence.get(0);
  if keys_table_existence != true {
    "2.3.2".to_string()
  } else {
    let key = "tbs_ver".to_string();
    let value = db.read("select value from taskboard_keys where key = $1;", &[&key]).await.unwrap();
    let value: String = value.get(0);
    value
  }
}

/// Обновляет базу данных до последней версии.
pub async fn upgrade_db(db: &Db, from_ver: &str) -> bool {
  match from_ver {
    // Обновление 1.0--2.3.2 -> 2.3.3
    //
    // В базе данных изменилась колонка background_color (boards) на background. В структуре данных Board строка `background_color` заменена на перечисление BoardBackground `background`.
    "2.3.2" => match db.write_mul(vec![
      ("alter table boards rename column background_color to background;", vec![]),
      (&format!("create table if not exists taskboard_keys (key varchar unique, value varchar); \
      insert into taskboard_keys values ('tbs_ver', '{}');", VERSION), vec![]),
    ]).await {
      Ok(_) => true,
      _ => false,
    },
    // Другие версии игнорируются.
    _ => true,
  }
}
