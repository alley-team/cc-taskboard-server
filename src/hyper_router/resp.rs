use hyper::http::Response;
use hyper::Body;

/// Формирует ответ из кода HTTP.
pub fn from_code_and_msg(code: u16, msg: Option<String>) -> Response<Body> {
  match msg {
    None => Response::builder().status(code).body(Body::empty()).unwrap(),
    Some(msg) => Response::builder()
      .header("Content-Type", "text/html; charset=utf-8")
      .status(code)
      .body(Body::from(msg))
      .unwrap(),
  }
}

// Выдаёт ошибук 400 BAD REQUEST.
// Выдаёт ошибку 401 UNAUTHORIZED.
// Выдаёт ошибку 402 PAYMENT REQUIRED.
// Выдаёт ошибку 404 NOT FOUND.
// Выдаёт ошибку 500 INTERNAL SERVER ERROR.
