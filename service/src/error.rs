//! Stable sanitized transport problems; never expose identity responses, bearer tokens or guest trap internals.
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct Problem {
    pub status: StatusCode,
    pub code: &'static str,
    pub detail: &'static str,
}
impl Problem {
    pub fn new(status: StatusCode, code: &'static str, detail: &'static str) -> Self {
        Self {
            status,
            code,
            detail,
        }
    }
    pub fn value(&self) -> Value {
        json!({"status":self.status.as_u16(),"code":self.code,"detail":self.detail})
    }
}
impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.value())).into_response();
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
        if self.status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                header::HeaderValue::from_static("Bearer realm=\"veyndo-app-runtime\""),
            );
        }
        if self.status == StatusCode::TOO_MANY_REQUESTS {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from_static("1"));
        }
        response
    }
}
pub type Result<T> = std::result::Result<T, Problem>;
