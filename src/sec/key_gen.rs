//! Отвечает за пароли.

use passwords::{PasswordGenerator, hasher::{bcrypt, gen_salt}};

/// Генерирует пароль, строго соответствующий заданным условиям.
pub fn generate_strong(length: usize) -> Result<String, &'static str> {
  let pg = PasswordGenerator {
    length,
    numbers: true,
    lowercase_letters: true,
    uppercase_letters: true,
    symbols: true,
    strict: true,
    exclude_similar_characters: true,
    spaces: false,
  };
  pg.generate_one()
}

/// Солит пароль, подготавливая к хранению в базе данных.
pub fn salt_pass(pass: String) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
  let salt = Vec::from(gen_salt());
  let salted_pass = Vec::from(bcrypt(10, &salt, &pass)?);
  Ok((salt, salted_pass))
}

/// Проверяет правильность пароля.
pub fn check_pass(salt: Vec<u8>, salted_pass: Vec<u8>, guessed_pass: &String) -> bool {
  salted_pass == bcrypt(10, &salt, &guessed_pass).unwrap()
}
