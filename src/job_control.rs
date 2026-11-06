//! Original merchant job-control transaction: row/CAS fence, explicit uncertainty review, required retry admission and atomic outbox.
use crate::{Scope, jobs};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, postgres::PgRow};
use std::{future::Future, pin::Pin};
pub type RetryFuture<'a, E> = Pin<Box<dyn Future<Output = Result<String, E>> + Send + 'a>>;
/// All identity facts come from the trusted authenticated host, never request-body actor fields.
pub struct Context<'a> {
    pub scope: Scope<'a>,
    pub actor: Option<&'a str>,
    pub package_digest: &'a str,
}
#[derive(Debug)]
pub enum ControlError<E> {
    Database(sqlx::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    NotFound,
    Hook(E),
}
impl<E> From<sqlx::Error> for ControlError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// The mandatory retry hook must validate the current installed job action, current
/// primary/additional actor rights and stored input. Return its admitted queued-event kind.
/// It runs while the original row lock is held. Every error requires caller rollback.
pub trait RetryAdmission<'db>: Sync {
    type Error: Send;
    fn admit<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        row: &'a PgRow,
    ) -> RetryFuture<'a, Self::Error>
    where
        'db: 'a;
}
/// Does not authenticate, begin or commit. The host first admits merchant/control/package access.
pub async fn control<'a, 'db: 'a, H: RetryAdmission<'db> + 'a>(
    tx: &'a mut Transaction<'db, Postgres>,
    context: &Context<'_>,
    id: &str,
    input: &Value,
    host: &'a H,
) -> Result<Value, ControlError<H::Error>> {
    let scope = context.scope;
    let row = sqlx::query("SELECT action,actor,permission,status,revision,input,lease_until<now() AS expired FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3 FOR UPDATE")
        .bind(scope.tenant).bind(scope.app).bind(id).fetch_optional(&mut **tx).await?
        .ok_or(ControlError::NotFound)?;
    if input["revision"].as_i64() != Some(row.get("revision")) {
        return Err(ControlError::Conflict("App job revision changed"));
    }
    let status: String = row.get("status");
    let expired = row.get::<Option<bool>, _>("expired").unwrap_or(false);
    let op = input["operation"]
        .as_str()
        .ok_or(ControlError::Invalid("Job operation required"))?;
    if op == "archive" {
        if !["succeeded", "failed", "cancelled"].contains(&status.as_str()) {
            return Err(ControlError::Conflict(
                "Only completed jobs can be archived",
            ));
        }
        sqlx::query("DELETE FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3")
            .bind(scope.tenant)
            .bind(scope.app)
            .bind(id)
            .execute(&mut **tx)
            .await?;
    } else {
        let next = match op {
            "cancel" if status == "queued" => "cancelled",
            "cancel" if status == "running" => "cancel_requested",
            "resolve_cancelled" if status == "uncertain" || expired => "cancelled",
            "retry" if status == "failed" || status == "uncertain" || expired => "queued",
            _ => return Err(ControlError::Conflict("Unsupported job transition")),
        };
        if ["retry", "resolve_cancelled"].contains(&op) && input["approveUnknownOutcome"] != true {
            return Err(ControlError::Conflict(
                "Confirm external outcome before retrying or resolving this job",
            ));
        }
        if op == "retry" {
            let event = host.admit(tx, &row).await.map_err(ControlError::Hook)?;
            jobs::signal(tx, scope, id, &event).await?;
        }
        let actor = context
            .actor
            .ok_or(ControlError::Invalid("Personal account required"))?;
        sqlx::query("UPDATE app_jobs SET status=$4,lease=CASE WHEN $4='cancel_requested' THEN lease ELSE NULL END,lease_until=CASE WHEN $4='cancel_requested' THEN lease_until ELSE NULL END,actor=CASE WHEN $4='queued' THEN $5 ELSE actor END,package_digest=CASE WHEN $4='queued' THEN $6 ELSE package_digest END,progress=CASE WHEN $4='queued' THEN 0 ELSE progress END,started_at=CASE WHEN $4='queued' THEN NULL ELSE started_at END,revision=revision+1,updated_at=now() WHERE tenant=$1 AND app=$2 AND id=$3")
            .bind(scope.tenant).bind(scope.app).bind(id).bind(next).bind(actor).bind(context.package_digest).execute(&mut **tx).await?;
    }
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.job_controlled',$2)")
        .bind(scope.tenant)
        .bind(json!({"app":scope.app,"jobId":id,"operation":op,"actor":context.actor}))
        .execute(&mut **tx)
        .await?;
    Ok(json!({"accepted":true}))
}
