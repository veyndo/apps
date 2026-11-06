//! Stable sanitized transport problems; never expose identity responses, bearer tokens or database details.
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
    pub detail: String,
    pub pointer: Option<String>,
}
impl Problem {
    pub fn new(status: StatusCode, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            code,
            detail: detail.into(),
            pointer: None,
        }
    }
    pub fn value(&self) -> Value {
        let mut value =
            json!({"status":self.status.as_u16(),"code":self.code,"detail":self.detail});
        if let Some(pointer) = &self.pointer {
            value["pointer"] = json!(pointer);
        }
        value
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
                header::HeaderValue::from_static("Bearer realm=\"veyndo-app-data\""),
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
