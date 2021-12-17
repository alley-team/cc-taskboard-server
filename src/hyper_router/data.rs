use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, serde::ts_seconds};

/// Сведения аутентификации администратора.
#[derive(Deserialize, Serialize)]
pub struct AdminAuth {
  pub key: String,
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

/// Сведения авторизации пользователя.
#[derive(Deserialize, Serialize)]
pub struct UserAuthData {
  pub login: String,
  pub pass: String,
  pub cc_key: String,
  pub tokens: Vec<Token>,
}

/// Набор цветов для раскраски компонента.
#[derive(Deserialize, Serialize)]
pub struct ColorSet {
  pub text_color: String,
  pub background_color: String,
}

/// Временные рамки для задач и подзадач.
#[derive(Deserialize, Serialize)]
pub struct Timelines {
  /// Предпочтительно закончить до X (даты и времени).
  pub preferred_time: String,
  /// Обязательно закончить до Y (даты и времени).
  pub max_time: String,
  /// Ожидаемое время выполнения задачи Z в минутах.
  pub expected_time: u32,
}

/// Тег.
#[derive(Deserialize, Serialize)]
pub struct Tag {
  pub title: String,
  pub color_set: ColorSet,
}

/// Подзадача.
#[derive(Deserialize, Serialize)]
pub struct Subtask {
  pub id: i64,
  pub title: String,
  pub executors: Vec<i64>,
  pub exec: bool,
  pub tags: Vec<Tag>,
  pub timelines: Timelines,
  pub color_set: ColorSet,
}

/// Задача.
#[derive(Deserialize, Serialize)]
pub struct Task {
  pub id: i64,
  pub title: String,
  pub executors: Vec<i64>,
  pub exec: bool,
  pub subtasks: Vec<Subtask>,
  pub notes: String,
  pub tags: Vec<Tag>,
  pub timelines: Timelines,
  pub color_set: ColorSet,
}

/// Доска.
#[derive(Deserialize, Serialize)]
pub struct Board {
  pub id: i64,
  pub title: String,
  pub tasks: Vec<Task>,
  pub color_set: ColorSet,
}

/// Страница.
#[derive(Deserialize, Serialize)]
pub struct Page {
  pub id: i64,
  pub title: String,
  pub boards: Vec<Board>,
  pub background_color: String,
}

/// Пользователь.
#[derive(Deserialize, Serialize)]
pub struct User {
  pub id: i64,
  pub shared_pages: Vec<i64>,
  pub auth_data: UserAuthData,
}

pub fn parse_admin_auth_key(bytes: hyper::body::Bytes) -> serde_json::Result<String> {
  Ok(serde_json::from_str::<AdminAuth>(&String::from_utf8(bytes.to_vec()).unwrap())?.key)
}
