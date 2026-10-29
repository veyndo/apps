//! Package approval binds all requested permissions to a reviewed digest; current actor rights apply before every installation path.
use super::*;
pub(super) fn decode(v: &Value) -> Result<Manifest> {
    let m = if let Some(s) = v["builtIn"].as_str().and_then(approval::built_in) {
        serde_json::from_str(s).map_err(|_| bad("Invalid built-in"))?
    } else {
        serde_json::from_value(v["manifest"].clone()).map_err(|e| bad(e.to_string()))?
    };
    validate(&m)?;
    approval::installation(&m)?;
    Ok(m)
}
pub(crate) fn actor_permissions(h: &RequestContext, m: &Manifest) -> Result<()> {
    if !m.lifecycles.is_empty() {
        auth::permit(h, "settings.write")?;
        for c in &m.lifecycles {
            auth::permit(
                h,
                if c.object == "order" {
                    "orders.write"
                } else {
                    "b2b.manage"
                },
            )?;
            for edge in &c.transitions {
                for permission in &edge.required_permissions {
                    auth::permit(h, permission)?;
                }
            }
        }
    }
    for p in &m.permissions {
        if let Some(scope) = credentials::scope(p) {
            auth::permit(h, scope)?;
        }
        if p == "knowledge.write" {
            auth::permit(h, "apps.manage")?;
        }
        if p == "payments.provider" {
            auth::permit(h, "payments.manage")?;
        }
        if p == "commerce.hooks" {
            auth::permit(h, "catalog.write")?;
            auth::permit(h, "settings.write")?;
        }
    }
    Ok(())
}
pub(super) fn approved(h: &RequestContext, m: &Manifest, v: &Value) -> Result<()> {
    actor_permissions(h, m)?;
    let mut expected = m.permissions.clone();
    expected.sort();
    expected.dedup();
    let Some(requested) = v["permissions"].as_array() else {
        return Err(conflict("Review and approve the package permissions"));
    };
    let mut actual = requested
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or(bad("Invalid consent permissions"))?;
    actual.sort();
    if v["approve"] != true || v["digest"] != approval::canonical_digest(m) || actual != expected {
        return Err(conflict(
            "Package approval must match its current digest and complete permission list",
        ));
    }
    Ok(())
}
pub(super) fn router() -> Router<App> {
    Router::new().secure_route("/api/apps/review", &[("POST", "apps.manage")], post(review))
}
async fn review(
    State(a): State<App>,
    h: RequestContext,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let mut t = merchant(&a, &h)?;
    if let Some(environment) = v["environment"].as_str().filter(|e| !e.is_empty()) {
        auth::permit(&h, "developer.manage")?;
        staging::owned(&a, &t, environment).await?;
        t = environment.into();
    }
    let m = decode(&v)?;
    actor_permissions(&h, &m)?;
    let (previous, upgrade) = upgrade_review::inspect(&a, &t, &m).await?;
    let old = previous.as_ref().map(|v| &v.permissions);
    let added = m
        .permissions
        .iter()
        .filter(|p| old.is_none_or(|v| !v.contains(p)))
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"app":m.id,"name":m.name,"version":m.version,"digest":approval::canonical_digest(&m),"permissions":m.permissions,"added":added,"previousVersion":previous.map(|v|v.version),"upgrade":upgrade}),
    ))
}
