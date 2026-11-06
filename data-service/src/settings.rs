//! Optional current personal/live administrator settings, using shared encrypted persistence; no secret export or package installation.
use crate::{
    App,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    settings_contract::{Read, Save},
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};
use veyndo_app_storage::{
    self as store,
    model::{self, manifest::Manifest},
    secrets::{self, SecretError},
};
struct Crypto<'a>(&'a veyndo_app_authority::vault::Vault);
impl secrets::Crypto for Crypto<'_> {
    type Error = String;
    fn seal(&self, b: &str, v: &str) -> std::result::Result<String, String> {
        self.0.seal(b, v)
    }
    fn open(&self, b: &str, v: &str) -> std::result::Result<String, String> {
        self.0.open(b, v)
    }
}
fn bad() -> Problem {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid_settings",
        "Use declared settings with explicit approval and the current package digest and revision",
    )
}
fn conflict() -> Problem {
    Problem::new(
        StatusCode::CONFLICT,
        "settings_conflict",
        "The App package or settings changed; reload and review before saving",
    )
}
fn error(e: store::settings::Error<String>) -> Problem {
    match e {
        store::settings::Error::Storage(SecretError::Conflict(_)) => conflict(),
        store::settings::Error::Model(_)
        | store::settings::Error::Storage(SecretError::Invalid(_)) => bad(),
        _ => unavailable(),
    }
}
fn admit<'a>(app: &'a App, p: &Principal, id: &str) -> Result<Crypto<'a>> {
    let Some(vault) = &app.config.settings else {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "settings_disabled",
            "Settings management is not enabled on this host",
        ));
    };
    crate::management::admit(p)?;
    if id.is_empty() || !veyndo_app_authority::database::identifier(id) {
        return Err(bad());
    }
    Ok(Crypto(vault))
}
async fn package(
    tx: &mut sqlx::PgConnection,
    tenant: &str,
    id: &str,
    write: bool,
) -> Result<Manifest> {
    let scope = store::Scope { tenant, app: id };
    let raw = if write {
        store::packages::lock(tx, scope)
            .await
            .map_err(|_| unavailable())?
    } else {
        sqlx::query_scalar("SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active")
            .bind(tenant)
            .bind(id)
            .fetch_optional(tx)
            .await
            .map_err(|_| unavailable())?
    }
    .ok_or_else(conflict)?;
    let m: Manifest = serde_json::from_value(raw).map_err(|_| unavailable())?;
    if m.id != id {
        return Err(conflict());
    }
    model::settings::validate(&m.settings).map_err(|_| bad())?;
    Ok(m)
}
pub async fn read(app: &App, p: Principal, input: Read) -> Result<Value> {
    let crypto = admit(app, &p, &input.app)?;
    let _slot = app.execution.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "data_capacity",
            "The data service is busy; retry shortly",
        )
    })?;
    let mut tx = app.begin(&p).await?;
    let m = package(&mut tx, &p.tenant, &input.app, false).await?;
    let mut value = store::settings::read(
        &mut tx,
        store::Scope {
            tenant: &p.tenant,
            app: &input.app,
        },
        &m,
        &crypto,
    )
    .await
    .map_err(error)?;
    value["canManage"] = json!(true);
    value["encryptionReady"] = json!(true);
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(value)
}
pub async fn save(app: &App, p: Principal, input: Save) -> Result<Value> {
    let crypto = admit(app, &p, &input.app)?;
    if !input.approve
        || input.revision < 0
        || input.digest.len() != 64
        || !input
            .digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(bad());
    }
    let _slot = app.execution.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "data_capacity",
            "The data service is busy; retry shortly",
        )
    })?;
    let mut tx = app.begin_write(&p).await?;
    let m = package(&mut tx, &p.tenant, &input.app, true).await?;
    if model::manifest::canonical_digest(&m) != input.digest {
        return Err(conflict());
    }
    let revision = store::settings::save(
        &mut tx,
        store::Scope {
            tenant: &p.tenant,
            app: &input.app,
        },
        &m,
        &store::settings::Update {
            digest: &input.digest,
            revision: input.revision,
            actor: &p.subject,
        },
        &input.values,
        &crypto,
    )
    .await
    .map_err(error)?;
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(json!({"saved":true,"revision":revision}))
}
pub fn router() -> Router<App> {
    Router::new()
        .route("/v1/settings/read", post(http_read))
        .route("/v1/settings/save", post(http_save))
}
async fn http_read(
    State(app): State<App>,
    Extension(p): Extension<Principal>,
    body: std::result::Result<Json<Read>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>> {
    Ok(Json(read(&app, p, body.map_err(|_| bad())?.0).await?))
}
async fn http_save(
    State(app): State<App>,
    Extension(p): Extension<Principal>,
    body: std::result::Result<Json<Save>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>> {
    Ok(Json(save(&app, p, body.map_err(|_| bad())?.0).await?))
}
