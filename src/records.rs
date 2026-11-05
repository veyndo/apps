//! Original fixed-table record SQL; host admission, signed scope, field rules and commit stay with the caller.
use crate::{RecordKey, Scope};
use serde_json::Value;
use sqlx::PgConnection;

pub fn lock_key(key: RecordKey<'_>) -> String {
    format!(
        "{}:{}:{}:{}",
        key.scope.tenant, key.scope.app, key.entity, key.id
    )
}
pub async fn get(
    tx: &mut PgConnection,
    key: RecordKey<'_>,
    write: bool,
) -> Result<Option<Value>, sqlx::Error> {
    let statement = if write {
        "SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 AND id=$4 FOR UPDATE"
    } else {
        "SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 AND id=$4 FOR SHARE"
    };
    sqlx::query_scalar(statement)
        .bind(key.scope.tenant)
        .bind(key.scope.app)
        .bind(key.entity)
        .bind(key.id)
        .fetch_optional(tx)
        .await
}
pub async fn rows(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    entity: &str,
    limit: i64,
) -> Result<Vec<Value>, sqlx::Error> {
    sqlx::query_scalar("SELECT fields||jsonb_build_object('id',id,'revision',revision) FROM app_records WHERE tenant=$1 AND app=$2 AND entity=$3 ORDER BY id LIMIT $4 FOR UPDATE")
        .bind(scope.tenant).bind(scope.app).bind(entity).bind(limit).fetch_all(tx).await
}
/// The host locks and validates CAS before this primitive, in the same transaction.
pub async fn put(
    tx: &mut PgConnection,
    key: RecordKey<'_>,
    revision: i64,
    fields: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app_records(tenant,app,entity,id,revision,version,fields) VALUES($1,$2,$3,$4,$5,'',$6) ON CONFLICT(tenant,app,entity,id) DO UPDATE SET fields=EXCLUDED.fields,revision=EXCLUDED.revision")
        .bind(key.scope.tenant).bind(key.scope.app).bind(key.entity).bind(key.id).bind(revision).bind(fields).execute(tx).await?;
    Ok(())
}
pub async fn define(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    version: &str,
    entity: &str,
    definition: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app_models(tenant,app,entity,version,definition) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant,app,entity) DO UPDATE SET version=EXCLUDED.version,definition=EXCLUDED.definition")
        .bind(scope.tenant).bind(scope.app).bind(entity).bind(version).bind(definition).execute(tx).await?;
    Ok(())
}
pub async fn reset_preview(tx: &mut PgConnection, scope: Scope<'_>) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM app_records WHERE tenant=$1 AND app=$2")
        .bind(scope.tenant)
        .bind(scope.app)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_models WHERE tenant=$1 AND app=$2")
        .bind(scope.tenant)
        .bind(scope.app)
        .execute(tx)
        .await?;
    Ok(())
}
