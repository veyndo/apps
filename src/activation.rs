//! Original package state transaction and active-dependant fence; current identity, connected frontends, trust and commerce/lifecycle admission are required host ports.
use crate::{Scope, installation::HookFuture};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
pub struct Change<'a> {
    pub scope: Scope<'a>,
    pub active: bool,
    /// Preserve original late revision admission, after current branch-specific checks.
    pub revision: Option<i64>,
}
#[derive(Debug)]
pub enum ChangeError<E> {
    Database(sqlx::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    Host(E),
}
impl<E> From<sqlx::Error> for ChangeError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// Every hook is mandatory, with no approving defaults; all use the same caller transaction.
/// The host admits current management authority before calling this operation.
pub trait StateHooks<'db>: Sync {
    type Error: Send;
    /// Lock the host automation/configuration owner before the tenant/package lifecycle fences.
    fn prepare<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    /// Admit installed package/current actor/consent/trust/dependencies/commerce/native lifecycle.
    fn activating<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    /// Refuse any currently connected native frontend or equivalent host-owned dependency.
    fn deactivating<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
}
/// Existing active package dependency check. Caller must hold the tenant lifecycle fence,
/// shared with installation, before using this check to admit a mutation.
async fn unused(tx: &mut Transaction<'_, Postgres>, scope: Scope<'_>) -> Result<bool, sqlx::Error> {
    let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_packages p WHERE tenant=$1 AND active AND id<>$2 AND EXISTS(SELECT 1 FROM jsonb_array_elements(coalesce(p.manifest->'distribution'->'dependencies','[]'::jsonb)) d WHERE d->>'app'=$2))")
        .bind(scope.tenant).bind(scope.app).fetch_one(&mut **tx).await?;
    Ok(!used)
}
/// Caller owns commit/rollback. No record/version/credential deletion, schema change,
/// new event or implicit installation. Original same-state CAS still advances revision.
pub async fn set<'a, 'db: 'a, H: StateHooks<'db> + 'a>(
    tx: &'a mut Transaction<'db, Postgres>,
    change: &Change<'_>,
    host: &'a H,
) -> Result<Value, ChangeError<H::Error>> {
    host.prepare(tx).await.map_err(ChangeError::Host)?;
    let scope = change.scope;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,543))")
        .bind(scope.tenant)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(format!("{}:{}", scope.tenant, scope.app))
        .execute(&mut **tx)
        .await?;
    if change.active {
        host.activating(tx).await.map_err(ChangeError::Host)?;
    } else {
        host.deactivating(tx).await.map_err(ChangeError::Host)?;
        if !unused(tx, scope).await? {
            return Err(ChangeError::Conflict(
                "App is required by another active app",
            ));
        }
    }
    let revision = change
        .revision
        .ok_or(ChangeError::Invalid("revision required"))?;
    let changed=sqlx::query("UPDATE app_packages SET active=$1,revision=revision+1 WHERE tenant=$2 AND id=$3 AND revision=$4")
        .bind(change.active).bind(scope.tenant).bind(scope.app).bind(revision).execute(&mut **tx).await?.rows_affected();
    if changed != 1 {
        return Err(ChangeError::Conflict("App revision changed"));
    }
    Ok(json!({"active":change.active,"dataRetained":true}))
}
