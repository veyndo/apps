//! Digest-bound per-shop app secret rotation reuses platform authenticated encryption; no plaintext read endpoint.
use super::*;
const KINDS: [&str; 3] = ["service", "webhook", "outbound"];
fn applicable(m: &Manifest, kind: &str) -> bool {
    match kind {
        "service" => m.permissions.contains(&"service.call".into()),
        "webhook" => !m.webhooks.is_empty(),
        "outbound" => m.event_delivery.as_ref().is_some_and(|d| d.url.is_some()),
        _ => false,
    }
}
fn binding(t: &str, id: &str, kind: &str, digest: &str, revision: i64) -> String {
    format!("app:{t}:{id}:{kind}:{digest}:{revision}")
}
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/secrets",
            &[("GET", "apps.manage")],
            get(list),
        )
        .secure_route(
            "/api/apps/{id}/secrets/{kind}",
            &[("PUT", "apps.manage")],
            axum::routing::put(rotate),
        )
}
async fn list(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, false).await?;
    let secrets: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'revision',revision,'rotatedAt',rotated_at,'packageDigest',package_digest) FROM app_service_secrets WHERE tenant=$1 AND app=$2 ORDER BY kind").bind(&t).bind(id).fetch_all(&a.db).await?;
    Ok(Json(
        json!({"digest":approval::canonical_digest(&m),"kinds":KINDS.iter().filter(|k|applicable(&m,k)).collect::<Vec<_>>(),"canManage":auth::permit(&h,"team.manage").is_ok() && h.principal.user.as_deref().is_some_and(|u|u!="bootstrap") && staging::parent(&a,&t).await?.is_none(),"secrets":secrets,"encryptionReady":vendune::inference::settings::encryption_ready()}),
    ))
}
async fn rotate(
    State(a): State<App>,
    h: RequestContext,
    Path((id, kind)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    auth::permit(&h, "team.manage")?;
    let actor = h
        .principal
        .user
        .as_deref()
        .filter(|u| *u != "bootstrap")
        .ok_or(bad("Personal account required"))?;
    if !KINDS.contains(&kind.as_str()) || staging::parent(&a, &t).await?.is_some() {
        return Err(bad("Secret kind or live workspace is invalid"));
    }
    let m = package(&a, &t, &id, true).await?;
    if !applicable(&m, &kind) {
        return Err(bad("Secret kind is not declared by this app"));
    }
    let digest = approval::canonical_digest(&m);
    let revision = v["revision"]
        .as_i64()
        .filter(|v| *v >= 0)
        .ok_or(bad("Secret revision required"))?;
    if v["approve"] != true || v["digest"] != digest {
        return Err(conflict(
            "Approve the current package digest before rotating its secret",
        ));
    }
    let secret = v["secret"]
        .as_str()
        .filter(|s| (32..=4096).contains(&s.len()) && !s.chars().any(char::is_control))
        .ok_or(bad(
            "Secret must be 32..4096 characters without control characters",
        ))?;
    let encrypted =
        vendune::inference::settings::seal(&binding(&t, &id, &kind, &digest, revision + 1), secret)
            .map_err(|_| {
                Error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Platform secret encryption is unavailable".into(),
                )
            })?;
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,844))")
        .bind(format!("{t}:{id}:{kind}"))
        .execute(&mut *tx)
        .await?;
    let current: Option<i64> = sqlx::query_scalar("SELECT revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3 FOR UPDATE").bind(&t).bind(&id).bind(&kind).fetch_optional(&mut *tx).await?;
    if current.unwrap_or(0) != revision {
        return Err(conflict(
            "Secret was rotated elsewhere; reload its revision",
        ));
    }
    let active_digest: String = sqlx::query_scalar(
        "SELECT digest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(&t)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await?;
    if digest != active_digest {
        return Err(conflict("App package changed before secret rotation"));
    }
    sqlx::query("INSERT INTO app_service_secrets(tenant,app,kind,ciphertext,package_digest,revision,actor) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(tenant,app,kind) DO UPDATE SET ciphertext=EXCLUDED.ciphertext,package_digest=EXCLUDED.package_digest,revision=EXCLUDED.revision,actor=EXCLUDED.actor,rotated_at=now()").bind(&t).bind(&id).bind(&kind).bind(encrypted).bind(&digest).bind(revision+1).bind(actor).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.secret_rotated',$2)")
        .bind(t)
        .bind(json!({"app":id,"kind":kind,"revision":revision+1,"actor":actor}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(json!({"rotated":true,"revision":revision+1})))
}
/// Existing operator fallbacks remain only when no tenant-specific value has ever been configured.
pub(super) async fn resolve(a: &App, t: &str, m: &Manifest, kind: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT ciphertext,package_digest,revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3").bind(t).bind(&m.id).bind(kind).fetch_optional(&a.db).await?;
    decrypt(row, t, m, kind)
}
/// A webhook verifies while holding the secret row, so rotation cannot race with intake/receipts.
pub(super) async fn resolve_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    m: &Manifest,
    kind: &str,
) -> Result<Option<String>> {
    let row=sqlx::query("SELECT ciphertext,package_digest,revision FROM app_service_secrets WHERE tenant=$1 AND app=$2 AND kind=$3 FOR SHARE").bind(t).bind(&m.id).bind(kind).fetch_optional(&mut **tx).await?;
    decrypt(row, t, m, kind)
}
fn decrypt(
    row: Option<sqlx::postgres::PgRow>,
    t: &str,
    m: &Manifest,
    kind: &str,
) -> Result<Option<String>> {
    let Some(row) = row else {
        return Ok(None);
    };
    let digest: String = row.get("package_digest");
    if digest != approval::canonical_digest(m) {
        return Err(conflict(
            "App secret belongs to an older package; review and rotate it for this version",
        ));
    }
    let secret = vendune::inference::settings::open(
        &binding(t, &m.id, kind, &digest, row.get("revision")),
        &row.get::<String, _>("ciphertext"),
    )
    .map_err(|_| {
        Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "App secret cannot be decrypted; check the platform key".into(),
        )
    })?;
    Ok(Some(secret))
}
