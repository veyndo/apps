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
#[derive(Clone)]
pub struct Principal(veyndo_app_authority::oauth::Principal);
impl std::ops::Deref for Principal {
    type Target = veyndo_app_authority::oauth::Principal;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn unauthorized() -> Problem {
    Problem::new(
        StatusCode::UNAUTHORIZED,
        "invalid_token",
        "An active access token for this service is required",
    )
}
impl App {
    async fn principal(&self, token: &str) -> Result<Principal> {
        let p = veyndo_app_authority::oauth::introspect(&self.http, &self.config.identity, token)
            .await
            .map_err(|e| match e {
                veyndo_app_authority::oauth::Error::InvalidToken => unauthorized(),
                veyndo_app_authority::oauth::Error::Unavailable => Problem::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "identity_unavailable",
                    "Access could not be checked; retry later",
                ),
            })?;
        if !p.merchant || !p.permits("apps.read") {
            return Err(Problem::new(
                StatusCode::FORBIDDEN,
                "missing_scope",
                "A current merchant identity with apps.read is required",
            ));
        }
        if [&p.tenant, &p.subject]
            .iter()
            .any(|s| s.is_empty() || !veyndo_app_authority::database::identifier(s))
        {
            return Err(Problem::new(
                StatusCode::FORBIDDEN,
                "identity_mapping",
                "Identity provider must supply canonical database tenant and actor IDs",
            ));
        }
        Ok(Principal(p))
    }
}
pub async fn admit(State(app): State<App>, mut request: Request, next: Next) -> Result<Response> {
    let mut origins = request.headers().get_all(header::ORIGIN).iter();
    if let Some(origin) = origins.next()
        && (origins.next().is_some()
            || !origin.to_str().ok().is_some_and(|o| {
                app.config
                    .identity
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
                "The request did not finish in time; the data service has no write operation",
            )
        })
}
