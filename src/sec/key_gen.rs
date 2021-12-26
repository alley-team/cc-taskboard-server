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

pub fn salt_pass(pass: String) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
  let salt = Vec::from(gen_salt());
  let salted_pass = Vec::from(bcrypt(10, &salt, &pass)?);
  Ok((salt, salted_pass))
}

pub fn check_pass(salt: Vec<u8>, salted_pass: Vec<u8>, guessed_pass: &String) -> bool {
  Vec::from(bcrypt(10, &salt, &guessed_pass).unwrap()) == salted_pass
}
