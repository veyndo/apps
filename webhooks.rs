//! Operator-signed incoming events: tenant-bound HMAC, five-minute freshness and atomic replay receipts.
use super::*;
use axum::body::Bytes;
use hmac::{Hmac, Mac};
use sha2::Sha256;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AppWebhook {
    pub id: String,
    pub action: String,
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if m.webhooks.len() > 8 {
        return Err(bad("Maximum eight app webhooks"));
    }
    for w in &m.webhooks {
        if !identifier(&w.id)
            || !ids.insert(&w.id)
            || !m
                .actions
                .iter()
                .any(|a| a.name == w.action && a.handler == "emit" && !a.public)
        {
            return Err(bad("Webhook requires a private emit action"));
        }
    }
    Ok(())
}
pub(super) fn router() -> Router<App> {
    Router::new().route("/webhooks/apps/{tenant}/{app}/{webhook}", post(receive))
}
fn verify(secret: &str, canonical: &str, signature: &str) -> Result<()> {
    let denied = || {
        Error(
            StatusCode::UNAUTHORIZED,
            "Invalid app webhook signature".into(),
        )
    };
    if secret.len() < 32 || signature.len() != 64 || !signature.is_ascii() {
        return Err(denied());
    }
    let bytes = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&signature[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| denied())?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| denied())?;
    mac.update(canonical.as_bytes());
    mac.verify_slice(&bytes).map_err(|_| denied())
}
async fn receive(
    State(a): State<App>,
    Path((t, id, webhook)): Path<(String, String, String)>,
    h: RequestContext,
    body: Bytes,
) -> Result<Json<Value>> {
    let denied = || {
        Error(
            StatusCode::UNAUTHORIZED,
            "App webhook authentication required".into(),
        )
    };
    if body.len() > 16384 {
        return Err(bad("Webhook body exceeds 16 KiB"));
    }
    let header = |key| h.get(key).and_then(|v| v.to_str().ok()).ok_or_else(denied);
    let stamp = header("x-app-timestamp")?;
    let time = stamp.parse::<i64>().map_err(|_| denied())?;
    if time.abs_diff(chrono::Utc::now().timestamp()) > 300 {
        return Err(denied());
    }
    let key = header("x-app-event-id")?;
    if key.is_empty()
        || key.len() > 100
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(denied());
    }
    let text = std::str::from_utf8(&body).map_err(|_| bad("Webhook JSON must be UTF-8"))?;
    let digest = hash(text);
    let config = &crate::runtime_config::get().webhook_keys;
    let secret = config[&t][&id].as_str().ok_or_else(denied)?;
    verify(
        secret,
        &format!("{t}\n{id}\n{webhook}\n{stamp}\n{key}\n{digest}"),
        header("x-app-signature")?,
    )?;
    if staging::parent(&a, &t).await?.is_some() {
        return Err(bad("Incoming webhooks are disabled in private sandboxes"));
    }
    let input: Value = serde_json::from_str(text).map_err(|_| bad("Invalid webhook JSON"))?;
    let mut tx = a.db.begin().await?;
    let row = sqlx::query(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE",
    )
    .bind(&t)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(Error(StatusCode::NOT_FOUND, "App unavailable".into()))?;
    let m: Manifest =
        serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid package"))?;
    let w = m
        .webhooks
        .iter()
        .find(|w| w.id == webhook)
        .ok_or(Error(StatusCode::NOT_FOUND, "Webhook not declared".into()))?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == w.action && a.handler == "emit")
        .ok_or(bad("Webhook action missing"))?;
    validate_input(&action.input_schema, &input)?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,19))")
        .bind(format!("{t}:{id}:{webhook}:{key}"))
        .execute(&mut *tx)
        .await?;
    if let Some(r)=sqlx::query("SELECT digest,event_id FROM app_webhook_receipts WHERE tenant=$1 AND app=$2 AND webhook=$3 AND request_key=$4").bind(&t).bind(&id).bind(&webhook).bind(key).fetch_optional(&mut *tx).await? {
        if r.get::<String,_>("digest")!=digest {return Err(conflict("Webhook event id already used for a different payload"));}
        return Ok(Json(json!({"accepted":true,"replayed":true,"eventId":r.get::<i64,_>("event_id")})));
    }
    let event: i64 =
        sqlx::query_scalar("INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3) RETURNING id")
            .bind(&t)
            .bind(format!("app.{id}.{}", w.action))
            .bind(input)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query("INSERT INTO app_webhook_receipts(tenant,app,webhook,request_key,digest,event_id) VALUES($1,$2,$3,$4,$5,$6)").bind(t).bind(id).bind(webhook).bind(key).bind(digest).bind(event).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"accepted":true,"replayed":false,"eventId":event}),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signature_binds_the_exact_message() {
        let secret = "synthetic-test-secret-at-least-32-bytes";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(b"fixture");
        let sig = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert!(verify(secret, "fixture", &sig).is_ok());
        assert!(verify(secret, "different shop", &sig).is_err());
        assert!(verify(secret, "fixture", &"é".repeat(32)).is_err());
    }
}
