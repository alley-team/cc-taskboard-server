use passwords::{PasswordGenerator, hasher::{bcrypt, gen_salt}};

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
  Ok(pg.generate_one()?)
}

pub fn salt_pass(pass: String) -> Result<(String, String), &'static str> {
  let salt = gen_salt();
  let salted_pass = String::from_utf8(Vec::from(bcrypt(10, &salt, &pass)?)).unwrap();
  Ok(salt, salted_pass)
}

pub fn check_pass(salt: String, salted_pass: String, guessed_pass: String) -> bool {
  String::from_utf8(Vec::from(bcrypt(10, &salt, &guessed_pass).unwrap())).unwrap() == salted_pass
}
