//! Отвечает за проверку цветовой гаммы.

use custom_error::custom_error;

custom_error!{pub IncorrectColor
  IncompatibleColorLen = "Цвет не представлен в виде #RRGGBB.",
  IncompatibleColorBeginning = "Цвет не начинается с #."
}

/// Проверяет цвет, передаваемый текстом, на соответствие требованиям.
pub fn validate_color(color: &str) -> Result<(), IncorrectColor> {
  if color.len() != 7 {
    return Err(IncorrectColor::IncompatibleColorLen);
  };
  if !color.starts_with('#') {
    return Err(IncorrectColor::IncompatibleColorBeginning);
  };
  Ok(())
}
