//! Original installed-package lock/read and canonical content fingerprint; validation/approval and transaction commit remain with the host.
use crate::Scope;
use serde_json::Value;
use sqlx::PgConnection;
pub fn digest(manifest: &Value) -> String {
    veyndo_app_model::manifest::value_digest(manifest)
}
/// Matches the install lock in shared mode. Holds the active scoped row until the caller commits or rolls back.
pub async fn lock(tx: &mut PgConnection, scope: Scope<'_>) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock_shared(hashtextextended($1,7))")
        .bind(format!("{}:{}", scope.tenant, scope.app))
        .execute(&mut *tx)
        .await?;
    sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(scope.tenant)
    .bind(scope.app)
    .fetch_optional(tx)
    .await
}
