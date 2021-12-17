//! По умолчанию доступны только ok_or_XXX для внешнего подключения, за исключением route_404.

use hyper::http::{Response, StatusCode, Result as HttpResult};
use hyper::Body;

/// Выдаёт ошибку 400 BAD_REQUEST.
pub fn route_400() -> Response<Body> {
  Response::builder()
    .status(StatusCode::BAD_REQUEST)
    .body(Body::empty()).unwrap()
}

/// Если функция вернула Result::Ok(resp), возвращает resp; иначе - 400.
pub fn ok_or_400(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_400(),
  }
}

/// Выдаёт ошибку 401 UNAUTHORIZED.
pub fn route_401() -> Response<Body> {
  Response::builder()
    .status(StatusCode::UNAUTHORIZED)
    .body(Body::empty()).unwrap()
}

/// Если функция вернула Result::Ok(resp), возвращает resp; иначе - 401.
pub fn ok_or_401(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_401(),
  }
}

/// Выдаёт ошибку 404 NOT FOUND.
pub fn route_404() -> Response<Body> {
  Response::builder()
    .status(StatusCode::NOT_FOUND)
    .body(Body::empty()).unwrap()
}

/// Если функция вернула Result::Ok(resp), возвращает resp; иначе - 404.
pub fn ok_or_404(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_404(),
  }
}

/// Выдаёт ошибку 500 INTERNAL SERVER ERROR.
pub fn route_500() -> Response<Body> {
  Response::builder()
    .status(StatusCode::INTERNAL_SERVER_ERROR)
    .body(Body::empty()).unwrap()
}

/// Если функция вернула Result::Ok(resp), возвращает resp; иначе - 500.
pub fn ok_or_500(response: HttpResult<Response<Body>>) -> Response<Body> {
  match response {
    Ok(resp) => resp,
    _ => route_500(),
  }
}
