//! Original atomic package installation sequence; mandatory host hooks retain compiler, commerce, migration and configuration authority.
use crate::{Scope, packages};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::{future::Future, pin::Pin};
/// Explicit Send future permits hosts to use original borrowed SQL transactions on multithreaded servers.
pub type HookFuture<'a, E> = Pin<Box<dyn Future<Output = Result<(), E>> + Send + 'a>>;

#[derive(Debug)]
pub enum InstallError<E> {
    Database(sqlx::Error),
    Invalid(&'static str),
    Conflict(&'static str),
    Hook(E),
}
impl<E> From<sqlx::Error> for InstallError<E> {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}
/// Host-admitted metadata serialized from its complete validated package type.
/// Preview is a trusted host decision, not a request-supplied bypass for version history.
pub struct Package<'a> {
    pub scope: Scope<'a>,
    pub version: &'a str,
    pub manifest: &'a Value,
    pub versions: bool,
}
/// Every hook is required and executes on the caller's original transaction.
/// Errors must abort that transaction. Hosts must not catch an error and commit partial effects.
pub trait InstallHooks<'db>: Sync {
    type Error: Send;
    fn prepare<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    fn admit<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    fn upgrade<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        previous: &'a Value,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    fn attach<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
    fn finish<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a;
}
/// No connection creation or commit. Keep the original host authorization and transaction scope.
pub async fn install<'a, 'db: 'a, H: InstallHooks<'db> + 'a>(
    tx: &'a mut Transaction<'db, Postgres>,
    package: &'a Package<'a>,
    hooks: &'a H,
) -> Result<Value, InstallError<H::Error>> {
    let scope = package.scope;
    if package.manifest["id"].as_str() != Some(scope.app)
        || package.manifest["version"].as_str() != Some(package.version)
    {
        return Err(InstallError::Invalid(
            "Package identity and version must match admitted manifest",
        ));
    }
    hooks.prepare(tx).await.map_err(InstallError::Hook)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,543))")
        .bind(scope.tenant)
        .execute(&mut **tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(format!("{}:{}", scope.tenant, scope.app))
        .execute(&mut **tx)
        .await?;
    let admitted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_packages WHERE tenant=$1 AND id=$2) OR (SELECT count(*) FROM app_packages WHERE tenant=$1)<128")
        .bind(scope.tenant).bind(scope.app).fetch_one(&mut **tx).await?;
    if !admitted {
        return Err(InstallError::Invalid(
            "Maximum 128 registered app packages per shop; update an existing app",
        ));
    }
    hooks.admit(tx).await.map_err(InstallError::Hook)?;
    let digest = packages::digest(package.manifest);
    if package.versions {
        let old: Option<String> = sqlx::query_scalar(
            "SELECT digest FROM app_versions WHERE tenant=$1 AND app=$2 AND version=$3",
        )
        .bind(scope.tenant)
        .bind(scope.app)
        .bind(package.version)
        .fetch_optional(&mut **tx)
        .await?;
        if old.is_some_and(|old| old != digest) {
            return Err(InstallError::Conflict("Published app version is immutable"));
        }
        let previous: Option<Value> =
            sqlx::query_scalar("SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2")
                .bind(scope.tenant)
                .bind(scope.app)
                .fetch_optional(&mut **tx)
                .await?;
        if let Some(previous) = previous {
            hooks
                .upgrade(tx, &previous)
                .await
                .map_err(InstallError::Hook)?;
        }
    }
    sqlx::query("INSERT INTO app_packages(tenant,id,version,manifest,digest) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,id) DO UPDATE SET version=EXCLUDED.version,manifest=EXCLUDED.manifest,digest=EXCLUDED.digest,active=true,revision=app_packages.revision+1")
        .bind(scope.tenant).bind(scope.app).bind(package.version).bind(package.manifest).bind(&digest).execute(&mut **tx).await?;
    hooks.attach(tx).await.map_err(InstallError::Hook)?;
    if package.versions {
        sqlx::query("INSERT INTO app_versions(tenant,app,version,digest,manifest) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(scope.tenant).bind(scope.app).bind(package.version).bind(&digest).bind(package.manifest).execute(&mut **tx).await?;
    }
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.installed',$2)")
        .bind(scope.tenant)
        .bind(json!({"app":scope.app,"version":package.version,"digest":digest}))
        .execute(&mut **tx)
        .await?;
    hooks.finish(tx).await.map_err(InstallError::Hook)?;
    Ok(json!({"installed":true,"id":scope.app,"version":package.version,"digest":digest}))
}
