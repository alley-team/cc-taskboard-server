use std::{env, io, io::Read, process, fs, boxed::Box};
use serde::{Deserialize, Serialize};

/// Данные приложения.
#[derive(Clone, Deserialize, Serialize)]
pub struct AppConfig {
  /// Конфигурация Postgres.
  pub pg_config: String,
  /// Ключ аутентификации администратора.
  pub admin_key: String,
  /// Порт прослушивания сервера.
  pub hyper_port: u16,
}

/// Запрашивает конфигурацию у пользователя.
fn stdin_setup() -> Result<AppConfig, Box<dyn std::error::Error>> {
  let stdin = io::stdin();
  println!("Привет! Это сервер CC TaskBoard. Прежде чем мы начнём, заполните несколько параметров.");
  println!("Введите имя пользователя PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
  let pg_config = String::from("host=localhost user='") + &buffer + &String::from("' password='");
  println!("Введите пароль PostgreSQL:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
  let pg_config = pg_config + &buffer + &String::from("'");
  println!("Введите номер порта сервера:");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let buffer = buffer.trim();
  let hyper_port: u16 = buffer.parse()?;
  println!("Введите ключ для аутентификации администратора (минимум 64 символа):");
  let mut buffer = String::new();
  stdin.read_line(&mut buffer)?;
  let admin_key = String::from(buffer.strip_suffix("\n").ok_or("")?);
  match admin_key.len() < 64 {
    true => Err(Box::new(io::Error::new(io::ErrorKind::Other, 
                                        "Длина ключа администратора меньше 64 символов."))),
    false => Ok(AppConfig { pg_config, admin_key, hyper_port }),
  }
}

/// Считывает информацию из данного файла.
///
/// WARNING Честно говоря, не лучший подход к проверке конфигурации на валидность, поскольку никто не проверяет строку Postgres.
fn parse_cfg_file(filepath: String) -> Result<AppConfig, Box<dyn std::error::Error>> {
  let mut file = fs::File::open(filepath)?;
  let mut buffer = String::new();
  file.read_to_string(&mut buffer)?;
  let conf: AppConfig = serde_json::from_str(&buffer)?;
  match conf.admin_key.len() < 64 {
    true => Err(Box::new(io::Error::new(io::ErrorKind::Other,
                                        "Длина ключа администратора меньше 64 символов."))),
    false => Ok(conf),
  }
}

/// Возвращает конфигурацию для запуска сервера.
pub fn get_config() -> AppConfig {
  match match env::args().len() == 1 {
    true => stdin_setup(),
    false => parse_cfg_file(env::args().nth(1).unwrap())
  } {
    Ok(conf) => {
      println!("Конфигурация загружена.");
      conf
    },
    Err(_) => {
      println!("Считать конфигурацию не удалось.");
      process::exit(1);
    },
  }
}
