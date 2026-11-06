//! Current installed-package/action authority for the original job control sequence; no worker, issuer or effect fallback.
use crate::{
    App,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    job_contract::Input,
};
use axum::http::StatusCode;
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction};
use veyndo_app_storage::{
    self as store,
    model::{self, manifest::Manifest},
};
pub fn bad() -> Problem {
    Problem::new(
        StatusCode::BAD_REQUEST,
        "invalid_job",
        "Use the bounded job operation and explicit current package/revision approval",
    )
}
pub fn conflict() -> Problem {
    Problem::new(
        StatusCode::CONFLICT,
        "job_changed",
        "The job or its installed package changed; reload and review it",
    )
}
pub fn missing() -> Problem {
    Problem::new(
        StatusCode::NOT_FOUND,
        "job_unavailable",
        "The App job is unavailable",
    )
}
pub fn validate(app: &App, input: &Input, op: &str) -> Result<()> {
    if !app.config.jobs {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "jobs_disabled",
            "Job supervision is not enabled on this host",
        ));
    }
    if op == "control"
        && (input.approve != Some(true)
            || !input
                .operation
                .as_deref()
                .is_some_and(|s| ["cancel", "retry", "resolve_cancelled", "archive"].contains(&s))
            || input.revision.is_none_or(|r| r < 1)
            || !input.package_digest.as_deref().is_some_and(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }))
    {
        return Err(bad());
    }
    model::schema::validate(
        &crate::job_contract::schema(op),
        &serde_json::to_value(input).map_err(|_| bad())?,
    )
    .map_err(|_| bad())
}
pub async fn package(
    tx: &mut sqlx::PgConnection,
    p: &Principal,
    id: &str,
    active: bool,
) -> Result<Manifest> {
    let raw: Option<Value> = sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND (NOT $3 OR active)",
    )
    .bind(&p.tenant)
    .bind(id)
    .bind(active)
    .fetch_optional(tx)
    .await
    .map_err(|_| unavailable())?;
    let m: Manifest =
        serde_json::from_value(raw.ok_or_else(missing)?).map_err(|_| unavailable())?;
    if m.id != id {
        return Err(unavailable());
    }
    Ok(m)
}
pub struct Admission<'a> {
    pub principal: &'a Principal,
    pub manifest: &'a Manifest,
}
impl<'db> store::job_control::RetryAdmission<'db> for Admission<'_> {
    type Error = Problem;
    fn admit<'a>(
        &'a self,
        _tx: &'a mut Transaction<'db, Postgres>,
        row: &'a sqlx::postgres::PgRow,
    ) -> store::job_control::RetryFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let action = self
                .manifest
                .actions
                .iter()
                .find(|a| a.name == row.get::<String, _>("action") && a.handler == "job")
                .ok_or_else(conflict)?;
            store::job_projection::rights(self.principal, action)?;
            model::schema::validate(&action.input_schema, &row.get::<Value, _>("input"))
                .map_err(|_| bad())?;
            Ok(store::job_projection::event_kind(
                self.manifest,
                &action.name,
            ))
        })
    }
}
pub fn error(e: store::job_control::ControlError<Problem>) -> Problem {
    match e {
        store::job_control::ControlError::Hook(e) => e,
        store::job_control::ControlError::Invalid(_) => bad(),
        store::job_control::ControlError::Conflict(_) => conflict(),
        store::job_control::ControlError::NotFound => missing(),
        _ => unavailable(),
    }
}
