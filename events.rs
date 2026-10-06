//! Durable at-least-once app events, retry leases and stable event idempotency keys.
use super::*;
pub(crate) async fn deliver_once(a: &App) -> Result<()> {
    let configured: Value = serde_json::from_str(&env::var("APP_SERVICES").unwrap_or("{}".into()))
        .map_err(|_| bad("Invalid app services"))?;
    let ids = configured
        .as_object()
        .map(|v| v.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if ids.is_empty() {
        return Ok(());
    }
    let row=sqlx::query("UPDATE app_deliveries SET state='running',attempts=attempts+1,lease_until=now()+interval '30 seconds' WHERE (tenant,app,event_id)=(SELECT d.tenant,d.app,d.event_id FROM app_deliveries d JOIN app_packages p ON p.tenant=d.tenant AND p.id=d.app AND p.active WHERE EXISTS(SELECT 1 FROM tenants t WHERE t.id=coalesce((SELECT live_tenant FROM shop_environments WHERE tenant=d.tenant),d.tenant) AND t.status='active') AND d.app=ANY($1) AND d.available_at<=now() AND (d.state='queued' OR d.state='running' AND d.lease_until<now()) ORDER BY d.event_id LIMIT 1 FOR UPDATE OF d SKIP LOCKED) RETURNING tenant,app,event_id,attempts").bind(ids).fetch_optional(&a.db).await?;
    let Some(r) = row else {
        return Ok(());
    };
    let t = r.get::<String, _>("tenant");
    let app = r.get::<String, _>("app");
    let id = r.get::<i64, _>("event_id");
    let event = sqlx::query("SELECT kind,data FROM outbox WHERE tenant=$1 AND id=$2")
        .bind(&t)
        .bind(id)
        .fetch_one(&a.db)
        .await?;
    let outcome=gateway::service_call(a,&t,&app,"events",&json!({"idempotencyKey":format!("{t}:{app}:{id}"),"eventId":id,"tenant":t,"kind":event.get::<String,_>("kind"),"data":event.get::<Value,_>("data")})).await;
    let attempt = r.get::<i32, _>("attempts");
    sqlx::query("UPDATE app_deliveries SET state=$1,error=$2,lease_until=NULL,available_at=now()+interval '10 seconds' WHERE tenant=$3 AND app=$4 AND event_id=$5 AND attempts=$6").bind(if outcome.is_ok(){"delivered"}else if attempt>=8{"failed"}else{"queued"}).bind(outcome.err().map(|e|e.1)).bind(t).bind(app).bind(id).bind(attempt).execute(&a.db).await?;
    Ok(())
}
pub(crate) async fn project_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    id: i64,
    kind: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO app_deliveries(tenant,app,event_id) SELECT tenant,id,$2 FROM app_packages WHERE tenant=$1 AND active AND manifest->'events' ? $3 ON CONFLICT DO NOTHING").bind(t).bind(id).bind(kind).execute(&mut **tx).await?;
    Ok(())
}
