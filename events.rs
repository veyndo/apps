//! Durable at-least-once app events, retry leases and stable event idempotency keys.
use super::*;
pub(crate) async fn deliver_once(a: &App) -> Result<bool> {
    let configured = &crate::runtime_config::get().services;
    let ids = configured
        .as_object()
        .map(|v| v.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let lease = Uuid::new_v4();
    let mut tx = a.db.begin().await?;
    let row=sqlx::query("WITH candidate AS (SELECT d.tenant,d.app,d.event_id FROM app_deliveries d JOIN app_packages p ON p.tenant=d.tenant AND p.id=d.app AND p.active JOIN tenants owner ON owner.id=d.tenant AND owner.status='active' WHERE NOT EXISTS(SELECT 1 FROM shop_environments s WHERE s.tenant=d.tenant) AND (d.app=ANY($1) OR p.manifest#>>'{eventDelivery,url}' IS NOT NULL) AND d.available_at<=now() AND (d.state='queued' OR d.state='running' AND d.lease_until<now()) AND (SELECT count(DISTINCT lease_token) FROM app_deliveries busy WHERE busy.tenant=d.tenant AND busy.state='running' AND busy.lease_until>now())<2 ORDER BY (SELECT max(last_attempt_at) FROM app_deliveries previous WHERE previous.tenant=d.tenant AND previous.app=d.app) NULLS FIRST,d.event_id LIMIT 1 FOR UPDATE OF d,owner SKIP LOCKED) UPDATE app_deliveries d SET state='running',attempts=attempts+1,lease_token=$2,last_attempt_at=now(),lease_until=now()+interval '30 seconds' FROM candidate c WHERE d.tenant=c.tenant AND d.app=c.app AND d.event_id=c.event_id RETURNING d.tenant,d.app,d.event_id").bind(ids).bind(lease).fetch_optional(&mut *tx).await?;
    let Some(r) = row else {
        return Ok(false);
    };
    let t = r.get::<String, _>("tenant");
    let app = r.get::<String, _>("app");
    let manifest: Value = sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(&t)
    .bind(&app)
    .fetch_one(&mut *tx)
    .await?;
    let m: Manifest = serde_json::from_value(manifest).map_err(|_| bad("Invalid event package"))?;
    let extra = event_contract::batch_size(&m) - 1;
    if extra > 0 {
        sqlx::query("WITH candidates AS (SELECT tenant,app,event_id FROM app_deliveries WHERE tenant=$1 AND app=$2 AND available_at<=now() AND (state='queued' OR state='running' AND lease_until<now()) ORDER BY event_id LIMIT $4 FOR UPDATE SKIP LOCKED) UPDATE app_deliveries d SET state='running',attempts=attempts+1,lease_token=$3,last_attempt_at=now(),lease_until=now()+interval '30 seconds' FROM candidates c WHERE d.tenant=c.tenant AND d.app=c.app AND d.event_id=c.event_id").bind(&t).bind(&app).bind(lease).bind(extra).execute(&mut *tx).await?;
    }
    let rows=sqlx::query("SELECT o.id,o.kind,o.data,d.attempts FROM app_deliveries d JOIN outbox o ON o.tenant=d.tenant AND o.id=d.event_id WHERE d.tenant=$1 AND d.app=$2 AND d.lease_token=$3 ORDER BY o.id").bind(&t).bind(&app).bind(lease).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let mut items = vec![];
    for event in &rows {
        let id = event.get::<i64, _>("id");
        let kind = event.get::<String, _>("kind");
        let data = event.get::<Value, _>("data");
        if event_projection::eligible(&m, &kind, &data) {
            items.push(json!({"idempotencyKey":format!("{t}:{app}:{id}"),"eventId":id,"tenant":t,"kind":kind,"data":event_projection::payload(&m,&kind,data)}));
        }
    }
    let payload = if event_contract::batch_size(&m) > 1 {
        json!({"apiVersion":"1","tenant":t,"app":app,"events":items})
    } else {
        items.first().cloned().unwrap_or(Value::Null)
    };
    // Unsubscribed or newly filtered deliveries complete without external effects; replay reapplies current permissions.
    let outcome = if items.is_empty() {
        Ok(json!({"skipped":true}))
    } else {
        event_contract::send(a, &t, &m, &payload).await
    };
    let state = if outcome.is_ok() {
        "delivered"
    } else {
        "queued"
    };
    let error = outcome.err().map(|e| e.1);
    for row in &rows {
        let attempt = row.get::<i32, _>("attempts");
        let delay = (1i64 << attempt.min(8)).min(300);
        sqlx::query("UPDATE app_deliveries SET state=$1,error=$2,lease_until=NULL,lease_token=NULL,available_at=now()+make_interval(secs=>$7) WHERE tenant=$3 AND app=$4 AND event_id=$5 AND lease_token=$6").bind(if error.is_some()&&attempt>=8{"failed"}else{state}).bind(&error).bind(&t).bind(&app).bind(row.get::<i64,_>("id")).bind(lease).bind(delay as f64).execute(&a.db).await?;
    }
    Ok(true)
}
pub(crate) async fn project_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    id: i64,
    kind: &str,
) -> Result<()> {
    let rows = sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active")
        .bind(t)
        .fetch_all(&mut **tx)
        .await?;
    let data: Value = sqlx::query_scalar("SELECT data FROM outbox WHERE tenant=$1 AND id=$2")
        .bind(t)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    for row in rows {
        let m: Manifest = serde_json::from_value(row.get("manifest"))
            .map_err(|_| bad("Invalid app event package"))?;
        if event_projection::eligible(&m, kind, &data) {
            sqlx::query("INSERT INTO app_deliveries(tenant,app,event_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(t).bind(&m.id).bind(id).execute(&mut **tx).await?;
        }
    }

    Ok(())
}
