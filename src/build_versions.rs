//! Immutable App Studio snapshots and recoverable history; actual compilation, identity, environment ownership and installation remain required host admission.
use crate::{Error, Scope};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Postgres, Row, Transaction, postgres::PgRow};
use std::{future::Future, pin::Pin};
/// Original development snapshot digest; do not substitute the installed-package canonical digest.
pub fn digest(manifest: &Value) -> String {
    format!("{:x}", Sha256::digest(manifest.to_string().as_bytes()))
}
/// Trusted host has completed manifest validation, language/source checks and private environment admission.
pub struct Build<'a> {
    pub scope: Scope<'a>,
    pub id: &'a str,
    pub environment: &'a str,
    pub version: &'a str,
    pub manifest: &'a Value,
    /// Retained prompt/evidence envelope already minimized by the host; never return as raw history.
    pub prompt: &'a str,
    pub provider: &'a str,
    pub model: Option<&'a str>,
    pub summary: &'a Value,
}
pub async fn save(tx: &mut PgConnection, build: &Build<'_>) -> Result<String, Error> {
    if build.manifest["id"].as_str() != Some(build.scope.app)
        || build.manifest["version"].as_str() != Some(build.version)
    {
        return Err(Error::Invalid(
            "Build identity and version must match admitted manifest",
        ));
    }
    let digest = digest(build.manifest);
    let changed=sqlx::query("INSERT INTO developer_builds(id,tenant,environment,app,version,digest,manifest,prompt,provider,model,summary) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(tenant,app,version) DO NOTHING")
        .bind(build.id).bind(build.scope.tenant).bind(build.environment).bind(build.scope.app).bind(build.version).bind(&digest).bind(build.manifest).bind(build.prompt).bind(build.provider).bind(build.model).bind(build.summary).execute(tx).await?.rows_affected();
    if changed != 1 {
        return Err(Error::Conflict(
            "Development version already exists; use a new version",
        ));
    }
    Ok(digest)
}
/// Trusted host rows: prompt is retained only for host evidence projection, never public history output.
pub async fn list(
    tx: &mut PgConnection,
    tenant: &str,
    archived: bool,
) -> Result<Vec<PgRow>, sqlx::Error> {
    let sql = if archived {
        "SELECT id,environment,app,version,digest,manifest,prompt,summary,state,provider,model,created_at FROM developer_builds WHERE tenant=$1 AND archived=true ORDER BY archived_at DESC,created_at DESC LIMIT 100"
    } else {
        "SELECT id,environment,app,version,digest,manifest,prompt,summary,state,provider,model,created_at FROM developer_builds WHERE tenant=$1 AND archived=false ORDER BY created_at DESC LIMIT 100"
    };
    sqlx::query(sql).bind(tenant).fetch_all(tx).await
}
/// Host admits current developer authority, live scope, app identifier and explicit archive/restore approval.
pub async fn archive(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    archived: bool,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("UPDATE developer_builds SET archived=$3, archived_at=CASE WHEN $3 THEN now() ELSE NULL END WHERE tenant=$1 AND app=$2 AND archived<>$3")
        .bind(scope.tenant).bind(scope.app).bind(archived).execute(tx).await?.rows_affected())
}
pub type StageFuture<'a, E> = Pin<Box<dyn Future<Output = Result<Value, E>> + Send + 'a>>;
/// Every implementation must admit owned private environment, typed package/compiler,
/// current actor scopes and the actual installer on this same transaction. No defaults.
pub trait StageAdmission<'db>: Sync {
    type Error: Send;
    fn install<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        environment: &'a str,
        manifest: &'a Value,
    ) -> StageFuture<'a, Self::Error>
    where
        'db: 'a;
}
#[derive(Debug)]
pub enum StageError<E> {
    Database(sqlx::Error),
    Unavailable,
    Conflict(&'static str),
    Host(E),
}
impl<E> From<sqlx::Error> for StageError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// Caller admits live developer access and explicit review first; owns rollback and commit.
pub async fn stage<'a, 'db: 'a, H: StageAdmission<'db> + 'a>(
    tx: &'a mut Transaction<'db, Postgres>,
    tenant: &str,
    id: &str,
    expected_digest: &Value,
    host: &'a H,
) -> Result<Value, StageError<H::Error>> {
    let row=sqlx::query("SELECT environment,manifest,digest FROM developer_builds WHERE tenant=$1 AND id=$2 AND archived=false FOR UPDATE")
        .bind(tenant).bind(id).fetch_optional(&mut **tx).await?.ok_or(StageError::Unavailable)?;
    if *expected_digest != row.get::<String, _>("digest") {
        return Err(StageError::Conflict("Build digest changed"));
    }
    let environment: String = row.get("environment");
    let manifest: Value = row.get("manifest");
    let result = host
        .install(tx, &environment, &manifest)
        .await
        .map_err(StageError::Host)?;
    sqlx::query("UPDATE developer_builds SET state='staged' WHERE tenant=$1 AND id=$2")
        .bind(tenant)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(result)
}
