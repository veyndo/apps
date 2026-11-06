//! Actor-private mutable autosaves with original CAS/quota fencing; no install, publication, identity resolution or schema creation.
use crate::{Error, model};
use serde_json::{Value, json};
use sqlx::{PgConnection, Postgres, Row, Transaction};
use std::{future::Future, pin::Pin};

/// Already authenticated tenant and actor, supplied by a trusted host, never a request-body grant.
#[derive(Clone, Copy)]
pub struct Owner<'a> {
    pub tenant: &'a str,
    pub actor: &'a str,
}
pub struct Draft<'a> {
    pub id: &'a str,
    /// Retain the submitted bytes/fields as JSON, including inherited/explicitly empty locales.
    pub manifest: &'a Value,
    pub environment: Option<&'a str>,
    pub revision: Option<i64>,
}
pub type AdmissionFuture<'a> = Pin<Box<dyn Future<Output = Result<bool, sqlx::Error>> + Send + 'a>>;
/// Required host ownership check on this connection after the original actor lock.
/// No default implementation; a tenant/environment name alone grants nothing.
pub trait EnvironmentAdmission: Sync {
    fn owned<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        tenant: &'a str,
        environment: &'a str,
    ) -> AdmissionFuture<'a>;
}

pub async fn list(tx: &mut PgConnection, owner: Owner<'_>) -> Result<Vec<Value>, sqlx::Error> {
    let rows=sqlx::query("SELECT id,revision,manifest,environment,updated_at::text AS updated_at FROM developer_drafts WHERE tenant=$1 AND actor_key=$2 ORDER BY updated_at DESC LIMIT 20")
        .bind(owner.tenant).bind(owner.actor).fetch_all(tx).await?;
    Ok(rows.iter().map(|r|json!({"id":r.get::<String,_>("id"),"revision":r.get::<i64,_>("revision"),"manifest":r.get::<Value,_>("manifest"),"environment":r.get::<Option<String>,_>("environment"),"updatedAt":r.get::<String,_>("updated_at")})).collect())
}

/// Caller retains current developer admission and transaction commit/rollback.
/// Drafts need strict shared wire decoding, not a compiled/installable package yet.
pub async fn save<H: EnvironmentAdmission>(
    tx: &mut Transaction<'_, Postgres>,
    owner: Owner<'_>,
    draft: &Draft<'_>,
    host: &H,
) -> Result<i64, Error> {
    if !model::declarations::identifier(draft.id) || draft.manifest.to_string().len() > 65536 {
        return Err(Error::Invalid("Draft identifier or 64 KiB budget invalid"));
    }
    let manifest: model::manifest::Manifest = serde_json::from_value(draft.manifest.clone())
        .map_err(|_| Error::Invalid("Draft must use the shared app contract"))?;
    if manifest.id != draft.id {
        return Err(Error::Invalid("Draft identity mismatch"));
    }
    let revision = draft
        .revision
        .filter(|r| *r >= 0)
        .ok_or(Error::Invalid("Draft revision required"))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,821))")
        .bind(format!("{}:{}", owner.tenant, owner.actor))
        .execute(&mut **tx)
        .await?;
    let environment = draft.environment.filter(|s| !s.is_empty());
    if let Some(stage) = environment
        && !host.owned(tx, owner.tenant, stage).await?
    {
        return Err(Error::Invalid("Draft sandbox does not belong to this shop"));
    }
    let current:Option<i64>=sqlx::query_scalar("SELECT revision FROM developer_drafts WHERE tenant=$1 AND actor_key=$2 AND id=$3 FOR UPDATE")
        .bind(owner.tenant).bind(owner.actor).bind(draft.id).fetch_optional(&mut **tx).await?;
    if current.unwrap_or(0) != revision {
        return Err(Error::Conflict(
            "Draft changed in another window; reload before saving",
        ));
    }
    if current.is_none()
        && sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM developer_drafts WHERE tenant=$1 AND actor_key=$2",
        )
        .bind(owner.tenant)
        .bind(owner.actor)
        .fetch_one(&mut **tx)
        .await?
            >= 20
    {
        return Err(Error::Invalid(
            "Maximum 20 private drafts; remove an old draft",
        ));
    }
    sqlx::query("INSERT INTO developer_drafts(tenant,actor_key,id,manifest,environment) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,actor_key,id) DO UPDATE SET manifest=EXCLUDED.manifest,environment=EXCLUDED.environment,revision=developer_drafts.revision+1,updated_at=now()")
        .bind(owner.tenant).bind(owner.actor).bind(draft.id).bind(draft.manifest).bind(environment).execute(&mut **tx).await?;
    // PostgreSQL rejects bigint exhaustion before this expression can overflow.
    Ok(revision + 1)
}

pub async fn remove(tx: &mut PgConnection, owner: Owner<'_>, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM developer_drafts WHERE tenant=$1 AND actor_key=$2 AND id=$3")
        .bind(owner.tenant)
        .bind(owner.actor)
        .bind(id)
        .execute(tx)
        .await?;
    Ok(())
}
