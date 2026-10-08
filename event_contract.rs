//! Explicit event filters, bounded batches and signed public HTTPS delivery extend the existing leased outbox, not a second queue.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventFilter {
    pub event: String,
    pub equals: HashMap<String, Value>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventDelivery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default = "single")]
    pub batch_size: u16,
}
fn single() -> u16 {
    1
}
pub(super) fn matches(pattern: &str, event: &str) -> bool {
    pattern == event
        || pattern
            .strip_suffix(".*")
            .is_some_and(|p| event.starts_with(&format!("{p}.")))
}
pub(super) fn filtered(m: &Manifest, kind: &str, data: &Value) -> bool {
    let filters = m
        .event_filters
        .iter()
        .filter(|f| matches(&f.event, kind))
        .collect::<Vec<_>>();
    if filters.is_empty() {
        return true;
    }
    let projected = event_projection::payload(m, kind, data.clone());
    filters
        .iter()
        .any(|f| f.equals.iter().all(|(k, v)| projected.get(k) == Some(v)))
}
pub(super) fn destination(url: &str) -> Result<reqwest::Url> {
    let u = reqwest::Url::parse(url).map_err(|_| bad("Invalid event destination URL"))?;
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.fragment().is_some()
        || u.query().is_some()
        || url.len() > 2048
    {
        return Err(bad(
            "Event destination requires public HTTPS without credentials, query or fragment",
        ));
    }
    if let Ok(ip) = u.host_str().unwrap_or("").parse()
        && !vendune::network_policy::public_address(ip)
    {
        return Err(bad("Private event destinations are prohibited"));
    }
    Ok(u)
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    if m.event_filters.len() > 24 {
        return Err(bad("Maximum 24 event filters"));
    }
    for f in &m.event_filters {
        if !m.events.contains(&f.event)
            || f.equals.is_empty()
            || f.equals.len() > 4
            || f.equals.iter().any(|(k, v)| {
                !identifier(k)
                    || !matches!(
                        v,
                        Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
                    )
                    || v.to_string().len() > 1000
            })
        {
            return Err(bad(
                "Filters require a declared event and 1..4 bounded scalar equalities",
            ));
        }
    }
    if let Some(d) = &m.event_delivery {
        if m.events.is_empty() || !(1..=25).contains(&d.batch_size) {
            return Err(bad(
                "Event delivery needs subscriptions and batchSize 1..25",
            ));
        }
        if let Some(url) = &d.url {
            destination(url)?;
            if !m.permissions.contains(&"events.send".into()) {
                return Err(bad("Outgoing webhooks require events.send permission"));
            }
        }
    }
    Ok(())
}
pub(super) fn batch_size(m: &Manifest) -> i64 {
    m.event_delivery
        .as_ref()
        .map(|d| d.batch_size as i64)
        .unwrap_or(1)
}
pub(super) async fn send(a: &App, t: &str, m: &Manifest, payload: &Value) -> Result<Value> {
    let Some(url) = m.event_delivery.as_ref().and_then(|d| d.url.as_deref()) else {
        return gateway::service_call_pinned(
            a,
            t,
            &m.id,
            "events",
            payload,
            Some(&approval::canonical_digest(m)),
        )
        .await;
    };
    if staging::parent(a, t).await?.is_some() {
        return Err(bad("Outgoing events are disabled in private sandboxes"));
    }
    let current = package(a, t, &m.id, true).await?;
    if approval::canonical_digest(&current) != approval::canonical_digest(m) {
        return Err(conflict("Event package changed before dispatch"));
    }
    let _cluster =
        crate::performance::cluster_lease::Lease::acquire(a, t, &format!("app:{}", m.id), 8)
            .await?;
    let _permit = a.app_limits.enter(t, &m.id)?;
    let secret = secrets::resolve(a, t, m, "outbound").await?.ok_or(Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Set an outgoing webhook signing secret before delivery".into(),
    ))?;
    let bytes = serde_json::to_vec(payload).map_err(|_| bad("Invalid event envelope"))?;
    if bytes.len() > 65536 {
        return Err(bad("Event batch exceeds 64 KiB; reduce batchSize"));
    }
    let stamp = chrono::Utc::now().timestamp().to_string();
    let key = hash(&format!(
        "{t}:{}:{}",
        m.id,
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let canonical = format!(
        "{t}\n{}\n{stamp}\n{key}\n{}",
        m.id,
        hash(std::str::from_utf8(&bytes).unwrap())
    );
    let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(secret.as_bytes())
        .map_err(|_| bad("Invalid signing secret"))?;
    hmac::Mac::update(&mut mac, canonical.as_bytes());
    let signature = hmac::Mac::finalize(mac)
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let response = egress::public_client(&destination(url)?)
        .await?
        .post(url)
        .header("content-type", "application/json")
        .header("x-tenant", t)
        .header("x-app-id", &m.id)
        .header("x-app-timestamp", stamp)
        .header("x-app-event-id", key)
        .header("x-app-signature", signature)
        .body(bytes)
        .send()
        .await
        .map_err(|_| {
            Error(
                StatusCode::BAD_GATEWAY,
                "Outgoing webhook unavailable".into(),
            )
        })?;
    if !response.status().is_success() {
        return Err(Error(
            StatusCode::BAD_GATEWAY,
            format!(
                "Outgoing webhook returned HTTP {}",
                response.status().as_u16()
            ),
        ));
    }
    Ok(json!({"accepted":true}))
}
