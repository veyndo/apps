//! Service job requests reuse the existing declared action gateway, actor checks, quotas and transactional outbox.
use super::*;
pub(super) async fn execute(a: &App, h: &RequestContext, m: &Manifest, v: &Value) -> Result<Value> {
    let contract: Value =
        serde_json::from_str(include_str!("../../extensions/sdk/host-exchange-v1.json"))
            .expect("bundled host exchange contract");
    input_schema::validate(&contract["operations"]["job_enqueue"]["inputSchema"], v)?;
    if !m.permissions.iter().any(|p| p == "jobs.write") {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App job permission required".into(),
        ));
    }
    credentials::permit(h, "jobs.write")?;
    let name = v["action"].as_str().unwrap();
    let key = v["requestKey"].as_str().unwrap();
    if key.is_empty() || key.chars().any(char::is_control) {
        return Err(bad(
            "A nonempty job request key without control characters is required",
        ));
    }
    let mut context = h.clone();
    context.insert(
        "idempotency-key",
        key.parse().map_err(|_| bad("Invalid job request key"))?,
    );
    // Preserve the authenticated principal. The gateway rechecks every action right,
    // schema, live-only admission and current package before creating the native job.
    gateway::invoke_job(a, &context, m, name, &v["input"]).await
}
