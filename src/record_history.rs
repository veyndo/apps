//! Bounded immutable app-record history and restore orchestration; current identity/entity admission and actual validated writes remain required host ports.
use crate::{Error, RecordKey, model::manifest::Manifest};
use serde_json::{Value, json};
use sqlx::{PgConnection, Postgres, Row, Transaction, postgres::PgRow};
use std::{future::Future, pin::Pin};

/// Original list-action rights are retained with each version, even if a later package narrows its declarations.
pub fn read_scopes(manifest: &Manifest, entity: &str) -> Value {
    json!(
        manifest
            .actions
            .iter()
            .filter(|a| a.entity.as_deref() == Some(entity) && a.handler == "list")
            .flat_map(|a| a
                .permission
                .iter()
                .chain(a.required_permissions.iter())
                .cloned())
            .collect::<Vec<_>>()
    )
}
pub struct Capture<'a> {
    pub key: RecordKey<'a>,
    pub revision: i64,
    pub fields: &'a Value,
    pub read_scopes: &'a Value,
}
/// Caller owns field validation, actor/source SQL context, package fence and transaction rollback.
pub async fn capture(tx: &mut PgConnection, snapshot: &Capture<'_>) -> Result<(), Error> {
    if snapshot.revision <= 0 {
        return Err(Error::Invalid(
            "App record history revision must be positive",
        ));
    }
    let key = snapshot.key;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,948))")
        .bind(format!("{}:{}", key.scope.tenant, key.scope.app))
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO app_record_versions(tenant,app,entity,record_id,revision,fields,read_scopes,actor,source) VALUES($1,$2,$3,$4,$5,$6,$7,nullif(current_setting('veyndo.actor',true),''),coalesce(nullif(current_setting('veyndo.source',true),''),'app.save')) ON CONFLICT DO NOTHING")
        .bind(key.scope.tenant).bind(key.scope.app).bind(key.entity).bind(key.id).bind(snapshot.revision)
        .bind(snapshot.fields).bind(snapshot.read_scopes).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 AND revision<=$5")
        .bind(key.scope.tenant).bind(key.scope.app).bind(key.entity).bind(key.id).bind(snapshot.revision-20).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM app_record_versions WHERE tenant=$1 AND app=$2 AND (entity,record_id,revision) IN (SELECT entity,record_id,revision FROM (SELECT entity,record_id,revision,sum(pg_column_size(fields)) OVER(ORDER BY created_at DESC,entity,record_id,revision DESC) AS size FROM app_record_versions WHERE tenant=$1 AND app=$2) r WHERE size>16777216)")
        .bind(key.scope.tenant).bind(key.scope.app).execute(tx).await?;
    Ok(())
}

/// Current trusted principal facts; historical rights are not granted by a package or request body.
pub trait ReadAuthority: Sync {
    fn allowed(&self, scope: &str) -> bool;
}
pub fn readable(scopes: &Value, authority: &impl ReadAuthority) -> bool {
    scopes.as_array().is_some_and(|scopes| {
        scopes
            .iter()
            .all(|s| s.as_str().is_some_and(|scope| authority.allowed(scope)))
    })
}
/// Trusted host rows, already filtered by historical read rights. The host must still
/// admit the current entity and project only its current declared fields before public output.
pub async fn list(
    tx: &mut PgConnection,
    key: RecordKey<'_>,
    authority: &impl ReadAuthority,
) -> Result<Vec<PgRow>, sqlx::Error> {
    let rows=sqlx::query("SELECT revision,fields,read_scopes,actor,source,created_at::text AS created_at FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 ORDER BY revision DESC LIMIT 20")
        .bind(key.scope.tenant).bind(key.scope.app).bind(key.entity).bind(key.id).fetch_all(tx).await?;
    Ok(rows
        .into_iter()
        .filter(|row| readable(&row.get::<Value, _>("read_scopes"), authority))
        .collect())
}
pub type RestoreFuture<'a, E> = Pin<Box<dyn Future<Output = Result<Value, E>> + Send + 'a>>;
/// Mandatory normal save path with current package/field/reference/lifecycle/CAS admission,
/// retained package fences, history and atomic event writes on this same transaction. No defaults.
pub trait RestoreHost<'db>: ReadAuthority {
    type Error: Send;
    fn save<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        key: RecordKey<'a>,
        fields: &'a Value,
    ) -> RestoreFuture<'a, Self::Error>
    where
        'db: 'a;
}
#[derive(Debug)]
pub enum RestoreError<E> {
    Database(sqlx::Error),
    Unavailable,
    Forbidden,
    Host(E),
}
impl<E> From<sqlx::Error> for RestoreError<E> {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}
/// Host admits current entity read/write rights, explicit approval and expected current revision first.
/// Restoring writes a new current revision; it never rewinds clocks or installs an older package.
pub async fn restore<'a, 'db: 'a, H: RestoreHost<'db> + 'a>(
    tx: &'a mut Transaction<'db, Postgres>,
    key: RecordKey<'a>,
    version: i64,
    host: &'a H,
) -> Result<Value, RestoreError<H::Error>> {
    let row=sqlx::query("SELECT fields,read_scopes FROM app_record_versions WHERE tenant=$1 AND app=$2 AND entity=$3 AND record_id=$4 AND revision=$5")
        .bind(key.scope.tenant).bind(key.scope.app).bind(key.entity).bind(key.id).bind(version)
        .fetch_optional(&mut **tx).await?.ok_or(RestoreError::Unavailable)?;
    if !readable(&row.get::<Value, _>("read_scopes"), host) {
        return Err(RestoreError::Forbidden);
    }
    let fields: Value = row.get("fields");
    host.save(tx, key, &fields)
        .await
        .map_err(RestoreError::Host)
}
