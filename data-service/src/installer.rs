//! Personal administrator package review/approval and original atomic installer; one current tenant, complete compiler and immutable versions.
use crate::{
    App,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    install_contract::{Install, Review},
    install_hooks::{self, Host},
    install_policy, management,
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};
use veyndo_app_storage::{self as store, installation::InstallError, model};
fn bad() -> Problem {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid_install",
        "Approve the exact compiled package, complete permission list and current revision",
    )
}
fn enabled<'a>(app: &'a App, p: &Principal) -> Result<&'a [String]> {
    management::admit(p)?;
    app.config.installs.as_deref().ok_or(Problem::new(
        StatusCode::FORBIDDEN,
        "install_disabled",
        "Package installation is not enabled on this host",
    ))
}
fn error(e: InstallError<Problem>) -> Problem {
    match e {
        InstallError::Hook(e) => e,
        InstallError::Conflict(_) => install_hooks::conflict(),
        InstallError::Invalid(_) => bad(),
        _ => unavailable(),
    }
}
pub async fn review(app: &App, p: Principal, input: Review) -> Result<Value> {
    let scopes = enabled(app, &p)?;
    let m = install_policy::compile(input.manifest, scopes)?;
    let _slot = app.execution.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "data_capacity",
            "The data service is busy; retry shortly",
        )
    })?;
    let mut tx = app.begin(&p).await?;
    let (revision, old) = install_hooks::prior(&mut tx, &p, &m).await?;
    let old = old
        .map(|v| install_policy::compile(v, scopes))
        .transpose()?;
    if let Some(old) = &old {
        install_hooks::compatible(old, &m)?;
    }
    store::models::review(
        &mut tx,
        store::Scope {
            tenant: &p.tenant,
            app: &m.id,
        },
        &m,
        &install_hooks::Compatible,
    )
    .await
    .map_err(install_hooks::model_error)?;
    let digest = model::manifest::canonical_digest(&m);
    let old_digest: Option<String> = sqlx::query_scalar(
        "SELECT digest FROM app_versions WHERE tenant=$1 AND app=$2 AND version=$3",
    )
    .bind(&p.tenant)
    .bind(&m.id)
    .bind(&m.version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    if old_digest.is_some_and(|v| v != digest) {
        return Err(install_hooks::conflict());
    }
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(
        json!({"app":m.id,"name":m.name,"version":m.version,"digest":digest,"permissions":m.permissions,"revision":revision,"previousVersion":old.map(|m|m.version),"models":m.entities.len(),"settings":m.settings.len()}),
    )
}
pub async fn install(app: &App, p: Principal, input: Install) -> Result<Value> {
    let scopes = enabled(app, &p)?;
    let m = install_policy::compile(input.manifest, scopes)?;
    let mut expected = m.permissions.clone();
    expected.sort();
    expected.dedup();
    let mut approved = input.permissions;
    approved.sort();
    if !input.approve
        || input.revision < 0
        || input.digest != model::manifest::canonical_digest(&m)
        || approved != expected
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
    let value = json!(m);
    let mut result = store::installation::install(
        &mut tx,
        &store::installation::Package {
            scope: store::Scope {
                tenant: &p.tenant,
                app: &m.id,
            },
            version: &m.version,
            manifest: &value,
            versions: true,
        },
        &Host {
            principal: &p,
            manifest: &m,
            revision: input.revision,
            scopes,
        },
    )
    .await
    .map_err(error)?;
    result["revision"] = json!(input.revision + 1);
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(result)
}
pub fn router() -> Router<App> {
    Router::new()
        .route("/v1/packages/review", post(http_review))
        .route("/v1/packages/install", post(http_install))
}
async fn http_review(
    State(app): State<App>,
    Extension(p): Extension<Principal>,
    body: std::result::Result<Json<Review>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>> {
    Ok(Json(review(&app, p, body.map_err(|_| bad())?.0).await?))
}
async fn http_install(
    State(app): State<App>,
    Extension(p): Extension<Principal>,
    body: std::result::Result<Json<Install>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<Value>> {
    Ok(Json(install(&app, p, body.map_err(|_| bad())?.0).await?))
}
