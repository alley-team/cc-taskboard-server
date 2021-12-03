use serde::{Deserialize, Serialize};

/// Сведения аутентификации администратора.
#[derive(Deserialize, Serialize)]
pub struct AdminAuth {
  pub key: String,
}

/// Сведения авторизации пользователя.
#[derive(Deserialize, Serialize)]
pub struct UserAuth {
  pub login: String,
  pub pass: String,
  pub cc_key: String,
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
  pub id: u64,
  pub title: String,
  pub executors: Vec<u64>,
  pub exec: bool,
  pub tags: Vec<Tag>,
  pub timelines: Timelines,
  pub color_set: ColorSet,
}

/// Задача.
#[derive(Deserialize, Serialize)]
pub struct Task {
  pub id: u64,
  pub title: String,
  pub executors: Vec<u64>,
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
  pub id: u64,
  pub title: String,
  pub tasks: Vec<Task>,
  pub color_set: ColorSet,
}

/// Страница.
#[derive(Deserialize, Serialize)]
pub struct Page {
  pub id: u64,
  pub title: String,
  pub boards: Vec<Board>,
  pub background_color: String,
}

/// Пользователь.
#[derive(Deserialize, Serialize)]
pub struct User {
  pub id: u64,
  pub shared_pages: Vec<u64>,
  pub auth_data: UserAuth,
}

fn parse_admin_auth_key(bytes: hyper::body::Bytes) -> serde_json::Result<String> {
  let auth: AdminAuth = serde_json::from_str(&String::from_utf8(bytes.to_vec()).unwrap())?;
  Ok(auth.key)
}
