//! Durable long actions piggyback on the existing outbox. Apps claim a fenced lease; unknown side effects are never retried automatically.
use super::*;
pub(super) async fn enqueue(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    action: &Action,
    v: &Value,
) -> Result<Value> {
    let t = merchant(a, h)?;
    if staging::parent(a, &t).await?.is_some() {
        return Err(bad("Long external jobs require a live workspace"));
    }
    let actor = h
        .principal
        .user
        .as_deref()
        .filter(|u| *u != "bootstrap")
        .ok_or(bad("Personal account required for long jobs"))?;
    let key = header(h, "idempotency-key")
        .filter(|s| !s.is_empty() && s.len() <= 100)
        .ok_or(bad("Idempotency-Key is required for a long app job"))?;
    if v.to_string().len() > 16384 {
        return Err(bad("App job input exceeds 16 KiB"));
    }
    let digest = hash(&v.to_string());
    let package_digest = approval::canonical_digest(m);
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,545))")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    if let Some(row)=sqlx::query("SELECT id,input_digest,action,package_digest FROM app_jobs WHERE tenant=$1 AND app=$2 AND request_key=$3").bind(&t).bind(&m.id).bind(key).fetch_optional(&mut *tx).await? {
        if row.get::<String,_>("input_digest")!=digest||row.get::<String,_>("action")!=action.name||row.get::<String,_>("package_digest")!=package_digest { return Err(conflict("Job idempotency key was used for a different request")); }
        return Ok(json!({"jobId":row.get::<String,_>("id"),"accepted":true,"reused":true}));
    }
    let (shop,app,retained):(i64,i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE status IN ('queued','running','cancel_requested','uncertain')),count(*) FILTER(WHERE app=$2 AND status IN ('queued','running','cancel_requested','uncertain')),count(*) FROM app_jobs WHERE tenant=$1").bind(&t).bind(&m.id).fetch_one(&mut *tx).await?;
    if shop >= 100 || app >= 10 || retained >= 1000 {
        return Err(Error(StatusCode::TOO_MANY_REQUESTS,"App job quota reached: 10 active/app, 100/shop, 1000 retained/shop; finish or archive completed jobs".into()));
    }
    let id = uid();
    sqlx::query("INSERT INTO app_jobs(tenant,app,id,action,actor,permission,package_digest,request_key,input_digest,input) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&t).bind(&m.id).bind(&id).bind(&action.name).bind(actor).bind(action.permission.as_deref().unwrap_or("apps.manage")).bind(package_digest).bind(key).bind(digest).bind(v).execute(&mut *tx).await?;
    signal(&mut tx, &t, &m.id, &id, &action.name).await?;
    tx.commit().await?;
    Ok(json!({"jobId":id,"accepted":true,"reused":false}))
}
async fn signal(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    app: &str,
    id: &str,
    action: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3)")
        .bind(t)
        .bind(format!("app.{app}.job_{action}"))
        .bind(json!({"jobId":id}))
        .execute(&mut **tx)
        .await?;
    Ok(())
}
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route("/api/apps/{id}/jobs", &[("GET", "apps.manage")], get(list))
        .secure_route(
            "/api/apps/{id}/jobs/{job}",
            &[("POST", "apps.manage")],
            post(control),
        )
}
async fn list(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    package(&a, &t, &id, false).await?;
    let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'action',action,'status',CASE WHEN status IN ('running','cancel_requested') AND lease_until<now() THEN 'uncertain' ELSE status END,'progress',progress,'message',message,'result',CASE WHEN status='succeeded' THEN result ELSE NULL END,'actor',actor,'revision',revision,'createdAt',created_at,'updatedAt',updated_at) FROM app_jobs WHERE tenant=$1 AND app=$2 ORDER BY created_at DESC,id DESC LIMIT 100").bind(&t).bind(&id).fetch_all(&a.db).await?;
    Ok(Json(json!({"jobs":rows})))
}
async fn control(
    State(a): State<App>,
    h: RequestContext,
    Path((app, id)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &app, true).await?;
    let mut tx = a.db.begin().await?;
    let row=sqlx::query("SELECT action,actor,permission,status,revision,input,lease_until<now() AS expired FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3 FOR UPDATE").bind(&t).bind(&app).bind(&id).fetch_optional(&mut *tx).await?.ok_or(Error(StatusCode::NOT_FOUND,"App job not found".into()))?;
    if v["revision"].as_i64() != Some(row.get("revision")) {
        return Err(conflict("App job revision changed"));
    }
    let status: String = row.get("status");
    let expired = row.get::<Option<bool>, _>("expired").unwrap_or(false);
    let op = v["operation"]
        .as_str()
        .ok_or(bad("Job operation required"))?;
    if op == "archive" {
        if !["succeeded", "failed", "cancelled"].contains(&status.as_str()) {
            return Err(conflict("Only completed jobs can be archived"));
        }
        sqlx::query("DELETE FROM app_jobs WHERE tenant=$1 AND app=$2 AND id=$3")
            .bind(&t)
            .bind(&app)
            .bind(&id)
            .execute(&mut *tx)
            .await?;
    } else {
        let next = match op {
            "cancel" if status == "queued" => "cancelled",
            "cancel" if status == "running" => "cancel_requested",
            "resolve_cancelled" if status == "uncertain" || expired => "cancelled",
            "retry" if status == "failed" || status == "uncertain" || expired => "queued",
            _ => return Err(conflict("Unsupported job transition")),
        };
        if ["retry", "resolve_cancelled"].contains(&op) && v["approveUnknownOutcome"] != true {
            return Err(conflict(
                "Confirm external outcome before retrying or resolving this job",
            ));
        }
        if op == "retry" {
            let action = m
                .actions
                .iter()
                .find(|a| a.name == row.get::<String, _>("action") && a.handler == "job")
                .ok_or(conflict("Job action is no longer installed"))?;
            auth::permit(&h, action.permission.as_deref().unwrap_or("apps.manage"))?;
            validate_input(&action.input_schema, &row.get::<Value, _>("input"))?;
            signal(&mut tx, &t, &app, &id, &action.name).await?;
        }
        sqlx::query("UPDATE app_jobs SET status=$4,lease=CASE WHEN $4='cancel_requested' THEN lease ELSE NULL END,lease_until=CASE WHEN $4='cancel_requested' THEN lease_until ELSE NULL END,actor=CASE WHEN $4='queued' THEN $5 ELSE actor END,package_digest=CASE WHEN $4='queued' THEN $6 ELSE package_digest END,progress=CASE WHEN $4='queued' THEN 0 ELSE progress END,started_at=CASE WHEN $4='queued' THEN NULL ELSE started_at END,revision=revision+1,updated_at=now() WHERE tenant=$1 AND app=$2 AND id=$3").bind(&t).bind(&app).bind(&id).bind(next).bind(h.principal.user.as_deref().ok_or(bad("Personal account required"))?).bind(approval::canonical_digest(&m)).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.job_controlled',$2)")
        .bind(&t)
        .bind(json!({"app":app,"jobId":id,"operation":op,"actor":h.principal.user}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"accepted":true})))
}
