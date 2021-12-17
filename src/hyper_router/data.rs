use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, serde::ts_seconds};

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
