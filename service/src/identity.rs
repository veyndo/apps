//! Fresh RFC 7662 introspection per protected request; only the fixed identity owner supplies current subject/tenant/scopes.
use crate::{
    App,
    error::{Problem, Result},
};
use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Clone)]
pub struct Principal {
    pub tenant: String,
    pub subject: String,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
struct Claims {
    active: bool,
    sub: Option<String>,
    tenant: Option<String>,
    scope: Option<String>,
    aud: Option<Audience>,
    exp: Option<u64>,
}
fn unauthorized() -> Problem {
    Problem::new(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "An active access token for this service is required",
    )
}
fn unavailable() -> Problem {
    Problem::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "identity_unavailable",
        "Access could not be checked; retry when the identity service is available",
    )
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:@".contains(&b))
}
impl App {
    async fn principal(&self, token: &str) -> Result<Principal> {
        let mut response = self
            .http
            .post(self.config.introspection_url.clone())
            .bearer_auth(self.config.introspection_credential.as_str())
            .form(&[
                ("token", token),
                ("token_type_hint", "access_token"),
                ("resource", self.config.audience.as_str()),
            ])
            .send()
            .await
            .map_err(|_| unavailable())?;
        if response.status() != StatusCode::OK {
            return Err(unavailable());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
            if body.len() + chunk.len() > 16384 {
                return Err(unavailable());
            }
            body.extend_from_slice(&chunk);
        }
        let c: Claims = serde_json::from_slice(&body).map_err(|_| unavailable())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| unavailable())?
            .as_secs();
        let audience = match c.aud {
            Some(Audience::One(a)) => a == self.config.audience,
            Some(Audience::Many(a)) => {
                a.len() <= 16 && a.iter().any(|a| a == &self.config.audience)
            }
            None => false,
        };
        if !c.active || !audience || c.exp.is_none_or(|t| t <= now) {
            return Err(unauthorized());
        }
        let (Some(subject), Some(tenant)) = (c.sub.filter(|s| id(s)), c.tenant.filter(|s| id(s)))
        else {
            return Err(unauthorized());
        };
        let scopes = c
            .scope
            .filter(|s| s.len() <= 4096)
            .ok_or_else(unauthorized)?;
        if !scopes
            .split_ascii_whitespace()
            .any(|s| s == "apps.evaluate")
        {
            return Err(Problem::new(
                StatusCode::FORBIDDEN,
                "missing_scope",
                "This operation requires apps.evaluate",
            ));
        }
        Ok(Principal { tenant, subject })
    }
}
pub async fn admit(State(app): State<App>, mut request: Request, next: Next) -> Result<Response> {
    let mut origins = request.headers().get_all(header::ORIGIN).iter();
    if let Some(origin) = origins.next()
        && (origins.next().is_some()
            || !origin.to_str().ok().is_some_and(|o| {
                app.config
                    .allowed_origins
                    .iter()
                    .any(|allowed| allowed == o)
            }))
    {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "origin_denied",
            "This origin is not allowed",
        ));
    }
    let mut values = request.headers().get_all(header::AUTHORIZATION).iter();
    let value = values
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthorized)?;
    if values.next().is_some() {
        return Err(unauthorized());
    }
    let token = value
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, token)| token)
        .filter(|s| (16..=4096).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or_else(unauthorized)?;
    let _admission = app.introspection.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "identity_capacity",
            "The access check service is busy; retry shortly",
        )
    })?;
    let principal = app.principal(token).await?;
    request.extensions_mut().insert(principal);
    tokio::time::timeout(std::time::Duration::from_secs(15), next.run(request))
        .await
        .map_err(|_| {
            Problem::new(
                StatusCode::GATEWAY_TIMEOUT,
                "request_timeout",
                "The request did not finish in time; no commerce effect was applied",
            )
        })
}
