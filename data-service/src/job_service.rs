//! Independent job supervision on the original shared history/control engine and signed transaction; execution stays with admitted producers/workers.
use crate::{
    App,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    job_contract::Input,
    job_host as host,
};
use axum::{
    Json, Router,
    extract::{Extension, State},
    http::StatusCode,
    routing::post,
};
use serde_json::{Value, json};
use sqlx::Row;
use veyndo_app_storage::{self as store, model::admission::ScopeAuthority};
fn capacity() -> Problem {
    Problem::new(
        StatusCode::TOO_MANY_REQUESTS,
        "data_capacity",
        "The data service is busy; retry shortly",
    )
}
pub async fn execute(app: &App, p: Principal, input: Input, op: &str) -> Result<Value> {
    host::validate(app, &input, op)?;
    if op == "control" {
        crate::management::admit(&p)?;
    } else if op == "list" {
        p.require_scope("apps.manage")?;
    }
    let _slot = app
        .execution
        .clone()
        .try_acquire_owned()
        .map_err(|_| capacity())?;
    let mut tx = if op == "control" {
        app.begin_write(&p).await?
    } else {
        app.begin(&p).await?
    };
    let scope = store::Scope {
        tenant: &p.tenant,
        app: &input.app,
    };
    if op == "control" {
        store::packages::lock(&mut tx, scope)
            .await
            .map_err(|_| unavailable())?
            .ok_or_else(host::missing)?;
    }
    let m = host::package(&mut tx, &p, &input.app, op != "list").await?;
    let digest = store::model::manifest::canonical_digest(&m);
    let result = match op {
        "list" => {
            let page = store::job_history::list(
                &mut tx,
                scope,
                &store::job_history::Query {
                    limit: input.limit.unwrap_or(100),
                    after: input.after.as_deref(),
                    group: input.group.as_deref(),
                },
            )
            .await
            .map_err(|e| match e {
                store::Error::Invalid(_) => host::bad(),
                store::Error::Conflict(_) => host::conflict(),
                _ => unavailable(),
            })?;
            let mut rows = page.rows;
            store::job_projection::history(&p, &m, &mut rows);
            json!({"jobs":rows,"limit":input.limit.unwrap_or(100),"hasMore":page.has_more,"nextCursor":page.next_cursor,"packageDigest":digest})
        }
        "detail" => {
            let row = store::job_history::detail(
                &mut tx,
                scope,
                input.id.as_deref().ok_or_else(host::bad)?,
            )
            .await
            .map_err(|_| unavailable())?
            .ok_or_else(host::missing)?;
            if p.subject != row.get::<String, _>("actor") && !p.permits("apps.manage") {
                return Err(host::missing());
            }
            if row.get::<String, _>("package_digest") != digest {
                return Err(host::conflict());
            }
            let action = m
                .actions
                .iter()
                .find(|a| a.name == row.get::<String, _>("action") && a.handler == "job")
                .ok_or_else(host::conflict)?;
            store::job_projection::rights(&p, action)?;
            row.get("data")
        }
        "control" => {
            if input.package_digest.as_deref() != Some(&digest) {
                return Err(host::conflict());
            }
            let v = json!({"operation":input.operation,"revision":input.revision,"approveUnknownOutcome":input.approve_unknown_outcome.unwrap_or(false)});
            store::job_control::control(
                &mut tx,
                &store::job_control::Context {
                    scope,
                    actor: Some(&p.subject),
                    package_digest: &digest,
                },
                input.id.as_deref().ok_or_else(host::bad)?,
                &v,
                &host::Admission {
                    principal: &p,
                    manifest: &m,
                },
            )
            .await
            .map_err(host::error)?
        }
        _ => return Err(host::bad()),
    };
    tx.commit().await.map_err(|_| unavailable())?;
    Ok(result)
}
pub fn router() -> Router<App> {
    Router::new()
        .route("/v1/jobs/list", post(list))
        .route("/v1/jobs/detail", post(detail))
        .route("/v1/jobs/control", post(control))
}
macro_rules! route {
    ($name:ident,$op:literal) => {
        async fn $name(
            State(app): State<App>,
            Extension(p): Extension<Principal>,
            body: std::result::Result<Json<Input>, axum::extract::rejection::JsonRejection>,
        ) -> Result<Json<Value>> {
            Ok(Json(
                execute(&app, p, body.map_err(|_| host::bad())?.0, $op).await?,
            ))
        }
    };
}
route!(list, "list");
route!(detail, "detail");
route!(control, "control");
