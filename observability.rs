//! Bounded metadata-only app telemetry, storage usage and explicitly approved cursor-based event replay.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/activity",
            &[("GET", "apps.manage")],
            get(activity),
        )
        .secure_route(
            "/api/apps/{id}/events/replay",
            &[("POST", "apps.manage")],
            post(replay),
        )
}
pub(super) async fn record(
    a: &App,
    h: &RequestContext,
    id: &str,
    name: &str,
    v: &Value,
    status: StatusCode,
    elapsed: u128,
) {
    let Ok(t) = tenant(h) else {
        return;
    };
    let actor = h.principal.user.as_deref().unwrap_or("anonymous");
    let inserted=sqlx::query("INSERT INTO app_action_log(tenant,app,action,actor,status,duration_ms,input_digest) SELECT $1,$2,$3,$4,$5,$6,$7 WHERE EXISTS(SELECT 1 FROM app_packages WHERE tenant=$1 AND id=$2)").bind(&t).bind(id).bind(name).bind(actor).bind(status.as_u16() as i32).bind(elapsed.min(i64::MAX as u128) as i64).bind(hash(&v.to_string())).execute(&a.db).await;
    if inserted.is_err() {
        eprintln!("App action metadata could not be recorded");
    }
    // Keyset retention is bounded and never stores payloads; keep the latest 1000 calls per app.
    let _=sqlx::query("DELETE FROM app_action_log WHERE tenant=$1 AND app=$2 AND id<(SELECT id FROM app_action_log WHERE tenant=$1 AND app=$2 ORDER BY id DESC OFFSET 999 LIMIT 1)").bind(t).bind(id).execute(&a.db).await;
}
async fn activity(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    package(&a, &t, &id, false).await?;
    let calls:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'action',action,'actor',actor,'status',status,'durationMs',duration_ms,'at',created_at) FROM app_action_log WHERE tenant=$1 AND app=$2 ORDER BY id DESC LIMIT 50").bind(&t).bind(&id).fetch_all(&a.db).await?;
    let deliveries:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('eventId',d.event_id,'kind',o.kind,'state',d.state,'attempts',d.attempts,'error',d.error,'lastAttemptAt',d.last_attempt_at) FROM app_deliveries d JOIN outbox o ON o.tenant=d.tenant AND o.id=d.event_id WHERE d.tenant=$1 AND d.app=$2 ORDER BY d.event_id DESC LIMIT 50").bind(&t).bind(&id).fetch_all(&a.db).await?;
    let usage:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('rows',rows,'bytes',bytes,'rowLimit',100000,'byteLimit',67108864) FROM app_storage_usage WHERE tenant=$1 AND app=$2").bind(t).bind(id).fetch_optional(&a.db).await?;
    Ok(Json(
        json!({"calls":calls,"deliveries":deliveries,"storage":usage,"delivery":"at-least-once","retentionCalls":1000}),
    ))
}
async fn replay(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    if v["approve"] != true {
        return Err(bad(
            "Approve event replay; external effects may occur again",
        ));
    }
    if staging::parent(&a, &t).await?.is_some() {
        return Err(bad("Private sandbox events cannot be delivered externally"));
    }
    let m = package(&a, &t, &id, true).await?;
    let after = v["after"]
        .as_i64()
        .filter(|n| *n >= 0)
        .ok_or(bad("Nonnegative event cursor required"))?;
    let limit = if v.get("limit").is_none() {
        50
    } else {
        v["limit"]
            .as_i64()
            .filter(|n| (1..=100).contains(n))
            .ok_or(bad("Replay limit must be 1..100"))?
    };
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,843))")
        .bind(format!("{t}:{id}"))
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query(
        "SELECT id,kind,data FROM outbox WHERE tenant=$1 AND id>$2 ORDER BY id LIMIT $3",
    )
    .bind(&t)
    .bind(after)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    let mut queued = 0;
    for r in &rows {
        let kind: String = r.get("kind");
        let data: Value = r.get("data");
        if event_projection::eligible(&m, &kind, &data) {
            queued+=sqlx::query("INSERT INTO app_deliveries(tenant,app,event_id) VALUES($1,$2,$3) ON CONFLICT(tenant,app,event_id) DO UPDATE SET state='queued',attempts=0,error=NULL,available_at=now(),lease_until=NULL,lease_token=NULL WHERE app_deliveries.state IN ('failed','delivered')").bind(&t).bind(&id).bind(r.get::<i64,_>("id")).execute(&mut *tx).await?.rows_affected();
        }
    }
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.replay_requested',$2)")
        .bind(t)
        .bind(json!({"app":id,"after":after,"queued":queued,"actor":h.principal.user}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"queued":queued,"nextCursor":rows.last().map(|r|r.get::<i64,_>("id")).unwrap_or(after),"hasMore":rows.len()==limit as usize}),
    ))
}
