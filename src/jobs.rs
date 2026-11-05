//! Original native app job persistence and transactional event handoff; no worker, principal or provider execution.
use crate::{Error, Scope};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

pub struct NewJob<'a> {
    pub scope: Scope<'a>,
    pub id: &'a str,
    pub action: &'a str,
    pub actor: &'a str,
    pub permission: &'a str,
    pub package_digest: &'a str,
    pub request_key: &'a str,
    pub input: &'a Value,
    pub event_kind: &'a str,
}
/// Caller owns the transaction and authorization. Job plus outbox either commit together or roll back together.
pub async fn enqueue(tx: &mut PgConnection, job: &NewJob<'_>) -> Result<Value, Error> {
    let (t, app) = (job.scope.tenant, job.scope.app);
    let digest = format!("{:x}", Sha256::digest(job.input.to_string().as_bytes()));
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,545))")
        .bind(t)
        .execute(&mut *tx)
        .await?;
    if let Some(row)=sqlx::query("SELECT id,input_digest,action,package_digest FROM app_jobs WHERE tenant=$1 AND app=$2 AND request_key=$3").bind(t).bind(app).bind(job.request_key).fetch_optional(&mut *tx).await? {
        if row.get::<String,_>("input_digest")!=digest || row.get::<String,_>("action")!=job.action || row.get::<String,_>("package_digest")!=job.package_digest {
            return Err(Error::Conflict("Job idempotency key was used for a different request"));
        }
        return Ok(json!({"jobId":row.get::<String,_>("id"),"accepted":true,"reused":true}));
    }
    let (shop,active,retained):(i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE status IN ('queued','running','cancel_requested','uncertain')),count(*) FILTER(WHERE app=$2 AND status IN ('queued','running','cancel_requested','uncertain')),count(*) FROM app_jobs WHERE tenant=$1").bind(t).bind(app).fetch_one(&mut *tx).await?;
    if shop >= 100 || active >= 10 || retained >= 1000 {
        return Err(Error::Quota(
            "App job quota reached: 10 active/app, 100/shop, 1000 retained/shop; finish or archive completed jobs",
        ));
    }
    sqlx::query("INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(t).bind(app).bind(job.id).bind(job.action).bind(job.actor).bind(job.permission).bind(job.package_digest).bind(job.request_key).bind(digest).bind(job.input).execute(&mut *tx).await?;
    signal(tx, job.scope, job.id, job.event_kind).await?;
    Ok(json!({"jobId":job.id,"accepted":true,"reused":false}))
}
pub async fn signal(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    id: &str,
    event: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3)")
        .bind(scope.tenant)
        .bind(event)
        .bind(json!({"jobId":id}))
        .execute(tx)
        .await?;
    Ok(())
}
pub async fn load(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    id: &str,
) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
    sqlx::query("SELECT action,actor,permission,package_digest,status,progress,lease,lease_until>now() AS valid,now()<started_at+interval '30 minutes' AS within_window,input,result FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3 FOR UPDATE")
        .bind(scope.tenant).bind(scope.app).bind(id).fetch_optional(tx).await
}
/// A queued job has exactly one winner. Expired/uncertain jobs are never reclaimed here.
pub async fn claim(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    id: &str,
    lease: &str,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE app_jobs SET status='running',lease=$4,lease_until=now()+interval '60 seconds',started_at=now(),updated_at=now(),revision=revision+1 WHERE tenant=$1 AND app=$2 AND id=$3 AND status='queued'")
        .bind(scope.tenant).bind(scope.app).bind(id).bind(lease).execute(tx).await?.rows_affected()==1)
}
