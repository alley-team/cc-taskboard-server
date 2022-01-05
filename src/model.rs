//! Модель данных приложения.

use custom_error::custom_error;
use hyper::Body;
use hyper::body::to_bytes;
use hyper::http::Request;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::sec::auth::UserCredentials;
use crate::setup::AppConfig;

type PgClient = Arc<Mutex<tokio_postgres::Client>>;

/// Объединяет окружение в одну структуру данных.
pub struct Workspace {
  /// Запрос, полученный от клиента. Содержит заголовки и тело.
  pub req: Request<Body>,
  /// Клиент PostgreSQL.
  pub cli: PgClient,
  /// Конфигурация сервера.
  pub cnf: AppConfig,
}

/// Набор цветов для раскраски компонента.
#[derive(Deserialize, Serialize)]
pub struct ColorSet {
  /// Цвет текста.
  pub text_color: String,
  /// Цвет фона.
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
  /// Текст тега.
  pub title: String,
  /// Раскраска тега.
  pub color_set: ColorSet,
}

/// Подзадача.
#[derive(Deserialize, Serialize)]
pub struct Subtask {
  /// Уникальный идентификатор подзадачи в пределах задачи.
  pub id: i64,
  /// Автор подзадачи.
  pub author: i64,
  /// Название подзадачи.
  pub title: String,
  /// Назначенные исполнители подзадачи.
  pub executors: Vec<i64>,
  /// Статус выполнения подзадачи (выполнена/не выполнена).
  pub exec: bool,
  /// Теги подзадачи.
  pub tags: Vec<Tag>,
  /// Временные рамки для подзадачи.
  pub timelines: Timelines,
  /// Раскраска подзадачи.
  pub color_set: ColorSet,
}

/// Задача.
#[derive(Deserialize, Serialize)]
pub struct Task {
  /// Уникальный идентификатор задачи в пределах карточки.
  pub id: i64,
  /// Автор задачи.
  pub author: i64,
  /// Название задачи.
  pub title: String,
  /// Назначенные исполнители задачи.
  pub executors: Vec<i64>,
  /// Статус выполнения задачи (выполнена/не выполнена).
  pub exec: bool,
  /// Список подзадач.
  pub subtasks: Vec<Subtask>,
  /// Заметки к задаче.
  pub notes: String,
  /// Теги задачи.
  pub tags: Vec<Tag>,
  /// Временные рамки для задачи.
  pub timelines: Timelines,
  /// Раскраска задачи.
  pub color_set: ColorSet,
}

/// Карточка.
#[derive(Deserialize, Serialize)]
pub struct Card {
  /// Уникальный идентификатор карточки в пределах доски.
  pub id: i64,
  /// Автор карточки.
  pub author: i64,
  /// Название карточки.
  pub title: String,
  /// Список задач.
  pub tasks: Vec<Task>,
  /// Раскраска карточки.
  pub color_set: ColorSet,
}

/// Доска.
#[derive(Deserialize, Serialize)]
pub struct Board {
  /// Уникальный идентификатор доски в базе данных.
  pub id: i64,
  /// Автор доски.
  pub author: i64,
  /// Список пользователей, у которых есть доступ к карточке.
  pub shared_with: Vec<i64>,
  /// Название доски.
  pub title: String,
  /// Список карточек.
  pub cards: Vec<Card>,
  /// Цвет фона.
  pub background_color: String,
}

/// Пользователь.
#[derive(Deserialize, Serialize)]
pub struct User {
  /// Идентификатор пользователя в базе данных.
  pub id: i64,
  /// Доступные доски.
  pub shared_boards: Vec<i64>,
  /// Сведения авторизации пользователя.
  pub user_creds: UserCredentials,
}

/// Возможные ошибки при извлечении данных из тела HTTP-запроса.
custom_error!{ ExtractionError
  FromBody = "Не удалось получить данные из тела запроса.",
  FromBytes = "Не удалось создать строку из набора байт тела запроса.",
  FromBase64 = "Не удалось декодировать данные из base64.",
  FromJson = "Не удалось десериализовать JSON."
}

/// Извлекает данные из тела HTTP-запроса.
///
/// Преобразует тело запроса в строку, декодирует кодировку base64, парсит результат в тип T и возвращает.
pub async fn extract<T>(req: Request<Body>) -> Result<T, ExtractionError>
  where
    T: DeserializeOwned,
{
  let body = req.into_body();
  let body = match to_bytes(body).await {
    Err(_) => return Err(ExtractionError::FromBody),
    Ok(v) => v,
  };
  let body = match String::from_utf8(body.to_vec()) {
    Err(_) => return Err(ExtractionError::FromBytes),
    Ok(v) => v.clone(),
  };
  let body = match base64::decode(&body) {
    Err(_) => return Err(ExtractionError::FromBase64),
    Ok(v) => match String::from_utf8(v) {
      Err(_) => return Err(ExtractionError::FromBase64),
      Ok(v) => v,
    },
  };
  let obj = serde_json::from_str::<T>(&body);
  match obj {
    Err(_) => return Err(ExtractionError::FromJson),
    Ok(v) => Ok(v),
  }
}
