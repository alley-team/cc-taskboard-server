use std::{env, io, io::Read, process, fs, boxed::Box, net::SocketAddr};
use serde::{Deserialize, Serialize};

/// Конфигурация приложения.
#[derive(Clone, Deserialize, Serialize)]
pub struct AppConfig {
  /// Конфигурация Postgres.
  pub pg: String,
  /// Ключ аутентификации администратора.
  pub admin_key: String,
  /// Порт прослушивания сервера.
  pub hyper_addr: SocketAddr,
}

impl AppConfig {
  /// Загружает конфигурацию.
  pub fn load() -> AppConfig {
    match match env::args().nth(1) {
      None => AppConfig::stdin_setup(),
      Some(filepath) => AppConfig::parse_cfg_file(filepath),
    } {
      Ok(conf) => {
        println!("Конфигурация загружена.");
        conf
      },
      _ => {
        eprintln!("Считать конфигурацию не удалось.");
        process::exit(1);
      },
    }
  }
  
  /// Запрашивает конфигурацию у пользователя.
  fn stdin_setup() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    println!("Привет! Это сервер CC TaskBoard. Прежде чем мы начнём, заполните несколько параметров.");
    println!("Введите имя пользователя PostgreSQL:");
    let mut buffer = String::new();
    stdin.read_line(&mut buffer)?;
    let buffer = buffer.trim();
    let pg = String::from("host=localhost user='") + &buffer + &String::from("' password='");
    println!("Введите пароль PostgreSQL:");
    let mut buffer = String::new();
    stdin.read_line(&mut buffer)?;
    let buffer = buffer.trim();
    let pg = pg + &buffer + &String::from("' connect_timeout=10 keepalives=0");
    println!("Введите IP-адрес и порт сервера:");
    let mut buffer = String::new();
    stdin.read_line(&mut buffer)?;
    let buffer = buffer.trim();
    let hyper_addr: SocketAddr = buffer.parse()?;
    println!("Введите ключ для аутентификации администратора (минимум 64 символа):");
    let mut buffer = String::new();
    stdin.read_line(&mut buffer)?;
    let admin_key = String::from(buffer.strip_suffix("\n").ok_or("")?);
    match admin_key.len() < 64 {
      true => Err(Box::new(io::Error::new(io::ErrorKind::Other, 
                                          "Длина ключа администратора меньше 64 символов."))),
      false => Ok(AppConfig { pg, admin_key, hyper_addr }),
    }
  }
  
  /// Считывает информацию из данного файла.
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
}

/// Возвращает конфигурацию для запуска сервера.
pub fn get_config() -> AppConfig {
  AppConfig::load()
}
