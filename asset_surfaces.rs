//! App file uploads reuse the product asset parser/store and require the current package plus either callback consent or a surface grant.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/surfaces/{surface}/assets",
            &[("POST", "catalog.write")],
            post(upload_surface),
        )
        .secure_route(
            "/api/apps/{id}/core/asset_upload",
            &[("POST", "catalog.write")],
            post(upload_callback),
        )
        .layer(axum::extract::DefaultBodyLimit::max(
            8 * 1024 * 1024 + 65536,
        ))
}
fn permitted(h: &RequestContext, m: &Manifest) -> Result<()> {
    if h.principal.app.as_deref().is_some_and(|id| id != m.id)
        || !m.permissions.iter().any(|p| p == "assets.write")
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App asset upload permission required".into(),
        ));
    }
    credentials::permit(h, "assets.write")?;
    auth::permit(h, "catalog.write")?;
    Ok(())
}
async fn upload_surface(
    State(a): State<App>,
    h: RequestContext,
    Path((id, surface)): Path<(String, String)>,
    multipart: axum::extract::Multipart,
) -> Result<Json<Value>> {
    let upload = assets::ingestion::parse(multipart, true).await?;
    let product = upload
        .product
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or(bad("Product ID required for asset upload"))?;
    let (m, s, ctx, actions) =
        surface_grants::load(&a, &h, &id, &surface, &json!({"grant":upload.grant})).await?;
    permitted(&h, &m)?;
    let action = m
        .actions
        .iter()
        .find(|x| {
            x.handler == "asset_upload"
                && actions
                    .as_array()
                    .is_some_and(|a| a.contains(&json!(x.name)))
        })
        .ok_or(Error(
            StatusCode::FORBIDDEN,
            "Asset upload is outside the surface grant".into(),
        ))?;
    surface_grants::bind_input(&m, &s, &ctx, &action.name, &json!({"productId":product}))?;
    assets::persist_app_upload(&a, &h, &product, upload)
        .await
        .map(Json)
}
async fn upload_callback(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    multipart: axum::extract::Multipart,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    permitted(&h, &m)?;
    if h.principal.app.is_none() {
        auth::permit(&h, "apps.manage")?;
    }
    let upload = assets::ingestion::parse(multipart, true).await?;
    if upload.grant.is_some() {
        return Err(bad(
            "Surface uploads must use their scoped surface endpoint",
        ));
    }
    let product = upload
        .product
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or(bad("Product ID required for asset upload"))?;
    assets::persist_app_upload(&a, &h, &product, upload)
        .await
        .map(Json)
}
