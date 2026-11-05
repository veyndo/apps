//! Lease-fenced native progress persistence, called only after host current-actor and package admission.
use crate::Scope;
use serde_json::Value;
use sqlx::PgConnection;
pub struct Progress<'a> {
    pub lease: &'a str,
    pub status: &'a str,
    pub progress: i32,
    pub message: &'a Value,
    pub result: &'a Value,
    pub checkpoint: Option<&'a Value>,
}
pub async fn advance(
    tx: &mut PgConnection,
    scope: Scope<'_>,
    id: &str,
    update: &Progress<'_>,
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE app_jobs SET status=$4,progress=$5,message=$6,result=CASE WHEN $4='succeeded' THEN $7 ELSE COALESCE($8,result) END,lease_until=CASE WHEN $4='running' THEN now()+interval '60 seconds' ELSE NULL END,revision=revision+1,updated_at=now() WHERE tenant=$1 AND app=$2 AND id=$3 AND lease=$9 AND lease_until>now() AND now()<started_at+interval '30 minutes' AND status IN ('running','cancel_requested') AND $4 IN ('running','succeeded','failed','cancelled') AND NOT(status='cancel_requested' AND $4='succeeded') AND ($4<>'cancelled' OR status='cancel_requested') AND $5>=progress AND $5 BETWEEN 0 AND 100")
        .bind(scope.tenant).bind(scope.app).bind(id).bind(update.status).bind(update.progress).bind(update.message).bind(update.result).bind(update.checkpoint).bind(update.lease).execute(tx).await?.rows_affected()==1)
}
