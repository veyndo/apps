//! Operator-pinned, bounded self-contained UI bundles use the same actor/context grant as app actions.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/surfaces/{surface}/bundle",
            &[("POST", "read")],
            post(load),
        )
        .route("/store-api/apps/{id}/surfaces/{surface}/bundle", post(load))
}
async fn load(
    State(a): State<App>,
    h: RequestContext,
    Path((id, surface)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let (m, s, _, _) = surface_grants::load(&a, &h, &id, &surface, &v).await?;
    let config = &crate::runtime_config::get().services[&id];
    if !approval::approved(&m, config) || native_views::is_native(&m, &s) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "External UI package is not approved".into(),
        ));
    }
    let digest = config["uiDigests"][&s.ui_path]
        .as_str()
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "App UI bundle requires an operator-approved SHA-256 digest".into(),
        ))?;
    let url = reqwest::Url::parse(
        &surfaces::surface_url(&id, &s.ui_path).ok_or(bad("App UI URL not configured"))?,
    )
    .map_err(|_| bad("Invalid UI URL"))?;
    let _permit = a.app_limits.enter(&tenant(&h)?, &id)?;
    let mut response = egress::client(&url)
        .await?
        .get(url)
        .send()
        .await
        .map_err(|_| Error(StatusCode::BAD_GATEWAY, "App UI bundle unavailable".into()))?;
    if !response.status().is_success()
        || !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"))
    {
        return Err(Error(
            StatusCode::BAD_GATEWAY,
            "App UI must return an HTML bundle".into(),
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| bad("Incomplete UI bundle"))?
    {
        if bytes.len() + chunk.len() > 1_048_576 {
            return Err(bad("App UI bundle exceeds 1 MiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let html = std::str::from_utf8(&bytes).map_err(|_| bad("App UI bundle must be UTF-8"))?;
    if hash(html) != digest.to_ascii_lowercase() {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App UI bundle changed; operator must review and approve its new digest".into(),
        ));
    }
    Ok(Json(
        json!({"html":html,"digest":digest,"sandbox":"opaque-origin; gateway-only actions"}),
    ))
}
