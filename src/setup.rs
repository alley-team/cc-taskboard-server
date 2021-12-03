use std::{env, io, process, fs, boxed::Box};

/// Данные приложения.
#[derive(Clone)]
pub struct AppConfig {
  /// Конфигурация Postgres.
  pub pg_config: String,
  /// Ключ аутентификации администратора.
  pub admin_key: String,
  /// Порт прослушивания сервера.
  pub hyper_port: u16,
}

/// Запрашивает конфигурацию у пользователя.
fn stdin_setup() -> Result<AppConfig, Box<dyn StdError>> {
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
    true => Err(Box::new(IOErr::new(IOErrKind::Other, "Длина ключа администратора меньше 64 символов."))),
    false => Ok(AppConfig { pg_config, admin_key, hyper_port }),
  }
}

/// Считывает информацию из данного файла.
fn parse_cfg_file(filepath: String) -> Result<AppConfig, Box<dyn std::error::Error>> {
  let file = fs::File::open(filepath)?;
  let mut buffer = String::new();
  file.read_to_string(&mut buffer)?;
  let conf: AppConfig = serde_json::from_str(&buffer)?;
  conf
}

/// Возвращает конфигурацию для запуска сервера.
pub fn get_config() -> AppConfig {
  match env.args().len() == 1 ? stdin_setup() : parse_cfg_file(env.args()[1]) {
    Ok(conf) => conf,
    Err(_) => {
      process::exit(1);
      AppConfig {}
    },
  }
}
