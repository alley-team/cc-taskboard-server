//! Отвечает за проверку цветовой гаммы.

use custom_error::custom_error;

custom_error!{pub IncorrectColor
  IncompatibleColorLen = "Цвет не представлен в виде #RRGGBB.",
  IncompatibleColorBeginning = "Цвет не начинается с #."
}

/// Проверяет цвет, передаваемый текстом, на соответствие требованиям.
pub fn validate_color(color: &str) -> Result<(), IncorrectColor> {
  if color.bytes().count() != 7 {
    return Err(IncorrectColor::IncompatibleColorLen);
  };
  if color.chars().nth(0) != Some('#') {
    return Err(IncorrectColor::IncompatibleColorBeginning);
  };
  Ok(())
}
