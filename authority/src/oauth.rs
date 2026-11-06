//! Fixed-endpoint, fresh bounded RFC 7662 introspection; only the identity owner supplies current authority.
use crate::oauth_config::Config;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
#[derive(Debug, Clone, Copy)]
pub enum Error {
    InvalidToken,
    Unavailable,
}
#[derive(Clone)]
pub struct Principal {
    pub tenant: String,
    pub subject: String,
    scopes: String,
    pub merchant: bool,
}
impl Principal {
    pub fn permits(&self, scope: &str) -> bool {
        self.scopes.split_ascii_whitespace().any(|s| s == scope)
    }
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
    #[serde(default)]
    merchant: bool,
}
fn unauthorized() -> Error {
    Error::InvalidToken
}
fn unavailable() -> Error {
    Error::Unavailable
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:@".contains(&b))
}
pub async fn introspect(
    http: &reqwest::Client,
    config: &Config,
    token: &str,
) -> Result<Principal, Error> {
    let mut response = http
        .post(config.introspection_url.clone())
        .bearer_auth(config.introspection_credential.as_str())
        .form(&[
            ("token", token),
            ("token_type_hint", "access_token"),
            ("resource", config.audience.as_str()),
        ])
        .send()
        .await
        .map_err(|_| unavailable())?;
    if response.status() != reqwest::StatusCode::OK {
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
        Some(Audience::One(a)) => a == config.audience,
        Some(Audience::Many(a)) => a.len() <= 16 && a.iter().any(|a| a == &config.audience),
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
    Ok(Principal {
        tenant,
        subject,
        scopes,
        merchant: c.merchant,
    })
}
