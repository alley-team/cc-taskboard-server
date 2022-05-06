//! Модель данных приложения.

use chrono::{DateTime, Utc, serde::ts_seconds};
use custom_error::custom_error;
use hyper::{Body, body::to_bytes, http::Request};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::psql_handler::Db;
use crate::sec::auth::UserCredentials;

custom_error!{ pub GetMutCardError{} = "Не удалось получить мутабельную карточку." }
custom_error!{ pub GetMutTaskError{} = "Не удалось получить мутабельную задачу." }
custom_error!{ pub GetMutSubtaskError{} = "Не удалось получить мутабельную подзадачу." }
custom_error!{ pub GetCardError{} = "Не удалось получить карточку." }
custom_error!{ pub GetTaskError{} = "Не удалось получить задачу." }
custom_error!{ pub GetSubtaskError{} = "Не удалось получить подзадачу." }
custom_error!{ pub CardRemoveError{} = "Не удалось удалить карточку." }
custom_error!{ pub TaskRemoveError{} = "Не удалось удалить задачу." }
custom_error!{ pub SubtaskRemoveError{} = "Не удалось удалить подзадачу." }

/// Объединяет окружение в одну структуру данных.
pub struct Workspace {
  /// Запрос, полученный от клиента. Содержит заголовки и тело.
  pub req: Request<Body>,
  /// Клиент PostgreSQL.
  pub db: Db,
}

/// Временные рамки для задач и подзадач.
#[derive(Clone, Deserialize, Serialize)]
pub struct Timelines {
  /// Предпочтительно закончить до X (даты и времени).
  #[serde(with = "ts_seconds")]
  pub preferred_time: DateTime<Utc>,
  /// Обязательно закончить до Y (даты и времени).
  #[serde(with = "ts_seconds")]
  pub max_time: DateTime<Utc>,
  /// Ожидаемое время выполнения задачи Z в минутах.
  pub expected_time: u32,
}

/// Метка.
#[derive(Clone, Deserialize, Serialize)]
pub struct Tag {
  /// Уникальный идентификатор тега в текущем списке тегов сущности.
  pub id: i64,
  /// Название метки.
  pub title: String,
  // Цвет текста метки.
  pub text_color: String,
  /// Цвет фона метки.
  pub background_color: String,
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
  // Цвет текста заголовка.
  pub header_text_color: String,
  /// Цвет фона заголовка.
  pub header_background_color: String,
  /// Цвет фона карточки.
  pub background_color: String,
}

/// Краткая информация о досках пользователя.
#[derive(Deserialize, Serialize)]
pub struct BoardsShort {
  /// Идентификатор доски.
  pub id: i64,
  /// Название доски.
  pub title: String,
  /// Цвет текста заголовка.
  pub header_text_color: String,
  /// Цвет фона заголовка.
  pub header_background_color: String,
}

/// Заголовок доски.
#[derive(Deserialize, Serialize)]
pub struct BoardHeader {
  /// Название доски.
  pub title: String,
  /// Цвет текста заголовка.
  pub header_text_color: String,
  /// Цвет фона заголовка.
  pub header_background_color: String,
}

/// Фон доски.
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub enum BoardBackground {
  /// Однотонный цвет.
  Color { color: String },
  /// Картинка с удалённого ресурса.
  URL { url: String }
}

/// Доска.
#[derive(Deserialize, Serialize)]
pub struct Board {
  /// Уникальный идентификатор доски в базе данных.
  pub id: i64,
  /// Заголовок доски.
  pub header: BoardHeader,
  /// Автор доски.
  pub author: i64,
  /// Список пользователей, у которых есть доступ к карточке.
  pub shared_with: Vec<i64>,
  /// Список карточек.
  pub cards: Vec<Card>,
  /// Фон доски.
  pub background: BoardBackground,
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

impl Task {
  /// Возвращает мутабельную ссылку на подзадачу.
  pub fn get_mut_subtask(&mut self, subtask_id: &i64) -> Result<&mut Subtask, GetMutSubtaskError> {
    let subtask_index: Option<usize> = self.subtasks.iter().position(|st| st.id == *subtask_id);
    if subtask_index.is_none() { return Err(GetMutSubtaskError{}); }
    let subtask_index: usize = subtask_index.unwrap();
    match self.subtasks.get_mut(subtask_index) {
      Some(subtask) => Ok(subtask),
      _ => Err(GetMutSubtaskError{}),
    }
  }
  
  /// Возвращает ссылку на подзадачу.
  pub fn get_subtask(&self, subtask_id: &i64) -> Result<&Subtask, GetSubtaskError> {
    let subtask_index: Option<usize> = self.subtasks.iter().position(|st| st.id == *subtask_id);
    if subtask_index.is_none() { return Err(GetSubtaskError{}); }
    let subtask_index: usize = subtask_index.unwrap();
    match self.subtasks.get(subtask_index) {
      Some(subtask) => Ok(subtask),
      _ => Err(GetSubtaskError{}),
    }
  }
  
  /// Удаляет и возвращает подзадачу.
  pub fn remove_subtask(&mut self, subtask_id: &i64) -> Result<Subtask, SubtaskRemoveError> {
    let subtask_index: Option<usize> = self.subtasks.iter().position(|st| st.id == *subtask_id);
    if subtask_index.is_none() { return Err(SubtaskRemoveError{}); }
    let subtask_index: usize = subtask_index.unwrap();
    Ok(self.subtasks.remove(subtask_index))
  }
}

impl Card {
  /// Возвращает мутабельную ссылку на задачу.
  pub fn get_mut_task(&mut self, task_id: &i64) -> Result<&mut Task, GetMutTaskError> {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(GetMutTaskError{}); }
    let task_index: usize = task_index.unwrap();
    match self.tasks.get_mut(task_index) {
      Some(task) => Ok(task),
      _ => Err(GetMutTaskError{}),
    }
  }
  
  /// Возвращает ссылку на задачу.
  pub fn get_task(&self, task_id: &i64) -> Result<&Task, GetTaskError> {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(GetTaskError{}); }
    let task_index: usize = task_index.unwrap();
    match self.tasks.get(task_index) {
      Some(task) => Ok(task),
      _ => Err(GetTaskError{}),
    }
  }
  
  /// Возвращает мутабельную ссылку на подзадачу одной из задач.
  pub fn get_mut_subtask(&mut self, task_id: &i64, subtask_id: &i64) 
    -> Result<&mut Subtask, GetMutSubtaskError>
  {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(GetMutSubtaskError{}); }
    let task_index: usize = task_index.unwrap();
    self.tasks[task_index].get_mut_subtask(subtask_id)
  }
  
  /// Возвращает ссылку на подзадачу одной из задач.
  pub fn get_subtask(&self, task_id: &i64, subtask_id: &i64) -> Result<&Subtask, GetSubtaskError> {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(GetSubtaskError{}); }
    let task_index: usize = task_index.unwrap();
    self.tasks[task_index].get_subtask(subtask_id)
  }
  
  /// Удаляет и возвращает задачу.
  pub fn remove_task(&mut self, task_id: &i64) -> Result<Task, TaskRemoveError> {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(TaskRemoveError{}); }
    let task_index: usize = task_index.unwrap();
    Ok(self.tasks.remove(task_index))
  }
  
  /// Удаляет и возвращает подзадачу одной из задач.
  pub fn remove_subtask(&mut self, task_id: &i64, subtask_id: &i64) 
    -> Result<Subtask, SubtaskRemoveError>
  {
    let task_index: Option<usize> = self.tasks.iter().position(|t| t.id == *task_id);
    if task_index.is_none() { return Err(SubtaskRemoveError{}); }
    let task_index: usize = task_index.unwrap();
    self.tasks[task_index].remove_subtask(subtask_id)
  }
}

pub trait Cards {
  fn get_mut_card(&mut self, card_id: &i64) -> Result<&mut Card, GetMutCardError>;
  fn get_mut_task(&mut self, card_id: &i64, task_id: &i64) -> Result<&mut Task, GetMutTaskError>;
  fn get_mut_subtask(&mut self, card_id: &i64, task_id: &i64, subtask_id: &i64)
    -> Result<&mut Subtask, GetMutSubtaskError>;
  fn get_card(&self, card_id: &i64) -> Result<&Card, GetCardError>;
  fn get_task(&self, card_id: &i64, task_id: &i64) -> Result<&Task, GetTaskError>;
  fn get_subtask(&self, card_id: &i64, task_id: &i64, subtask_id: &i64)
    -> Result<&Subtask, GetSubtaskError>;
  fn remove_card(&mut self, card_id: &i64) -> Result<Card, CardRemoveError>;
  fn remove_task(&mut self, card_id: &i64, task_id: &i64) -> Result<Task, TaskRemoveError>;
  fn remove_subtask(&mut self, card_id: &i64, task_id: &i64, subtask_id: &i64) 
    -> Result<Subtask, SubtaskRemoveError>;
}

impl Cards for Vec<Card> {
  /// Возвращает мутабельную ссылку на карточку.
  fn get_mut_card(&mut self, card_id: &i64) -> Result<&mut Card, GetMutCardError> {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetMutCardError{}); }
    let card_index: usize = card_index.unwrap();
    match self.get_mut(card_index) {
      Some(card) => Ok(card),
      _ => Err(GetMutCardError{}),
    }
  }
  
  /// Возвращает ссылку на карточку.
  fn get_card(&self, card_id: &i64) -> Result<&Card, GetCardError> {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetCardError{}); }
    let card_index: usize = card_index.unwrap();
    match self.get(card_index) {
      Some(card) => Ok(card),
      _ => Err(GetCardError{}),
    }
  }
  
  /// Возвращает мутабельную ссылку на задачу в одной из карточек.
  fn get_mut_task(&mut self, card_id: &i64, task_id: &i64)
    -> Result<&mut Task, GetMutTaskError>
  {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetMutTaskError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].get_mut_task(task_id)
  }
  
  /// Возвращает ссылку на задачу в одной из карточек.
  fn get_task(&self, card_id: &i64, task_id: &i64) -> Result<&Task, GetTaskError> {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetTaskError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].get_task(task_id)
  }
  
  /// Возвращает мутабельную ссылку на подзадачу.
  fn get_mut_subtask(&mut self, card_id: &i64, task_id: &i64, subtask_id: &i64) 
    -> Result<&mut Subtask, GetMutSubtaskError>
  {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetMutSubtaskError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].get_mut_subtask(task_id, subtask_id)
  }
  
  /// Возвращает ссылку на подзадачу.
  fn get_subtask(&self, card_id: &i64, task_id: &i64, subtask_id: &i64) 
    -> Result<&Subtask, GetSubtaskError>
  {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(GetSubtaskError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].get_subtask(task_id, subtask_id)
  }
  
  /// Удаляет и возвращает карточку.
  fn remove_card(&mut self, card_id: &i64) -> Result<Card, CardRemoveError> {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(CardRemoveError{}); }
    let card_index: usize = card_index.unwrap();
    Ok(self.remove(card_index))
  }
  
  /// Удаляет и возвращает задачу одной из карточек.
  fn remove_task(&mut self, card_id: &i64, task_id: &i64) -> Result<Task, TaskRemoveError> {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(TaskRemoveError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].remove_task(task_id)
  }
  
  /// Удаляет и возвращает подзадачу.
  fn remove_subtask(&mut self, card_id: &i64, task_id: &i64, subtask_id: &i64) 
    -> Result<Subtask, SubtaskRemoveError>
  {
    let card_index: Option<usize> = self.iter().position(|c| c.id == *card_id);
    if card_index.is_none() { return Err(SubtaskRemoveError{}); }
    let card_index: usize = card_index.unwrap();
    self[card_index].remove_subtask(task_id, subtask_id)
  }
}

// Возможные ошибки при извлечении данных из тела HTTP-запроса.
custom_error!{ pub ExtractionError
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
  let body = match to_bytes(req.into_body()).await {
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
  match serde_json::from_str::<T>(&body) {
    Err(_) => Err(ExtractionError::FromJson),
    Ok(v) => Ok(v),
  }
}
