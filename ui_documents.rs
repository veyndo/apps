//! Secret-free, operator-pinned guest code in a response-enforced opaque sandbox, never srcdoc.
use super::*;
use axum::response::{Html, IntoResponse, Response};
use tokio::sync::Semaphore;

// Only this handler can mark a response. Request paths/headers cannot opt out of Studio CSP.
#[derive(Clone)]
pub(crate) struct SandboxedAppDocument;
static CAPACITY: Semaphore = Semaphore::const_new(8);

pub(super) fn router() -> Router<App> {
    Router::new().route("/app-ui/{id}/{digest}", get(document))
}
async fn document(Path((id, digest)): Path<(String, String)>) -> Result<Response> {
    if !identifier(&id) || digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("Invalid app document"));
    }
    let config = crate::runtime_config::get().services.get(&id).ok_or(Error(
        StatusCode::NOT_FOUND,
        "App document unavailable".into(),
    ))?;
    // This endpoint serves static code only. No tenant, surface grant, cookie, context or
    // callback credential reaches the upstream. Every data/action call still uses the host grant.
    let path = config["uiDigests"]
        .as_object()
        .and_then(|pins| {
            pins.iter()
                .find(|(_, hash)| hash.as_str() == Some(digest.as_str()))
        })
        .map(|(path, _)| path)
        .ok_or(Error(
            StatusCode::NOT_FOUND,
            "App document is not pinned".into(),
        ))?;
    let _permit = CAPACITY.try_acquire().map_err(|_| {
        Error(
            StatusCode::TOO_MANY_REQUESTS,
            "App document capacity reached".into(),
        )
    })?;
    let url = reqwest::Url::parse(
        &surfaces::surface_url(&id, path).ok_or(bad("App UI URL not configured"))?,
    )
    .map_err(|_| bad("Invalid UI URL"))?;
    let html = ui_bundles::fetch(url, &digest).await?;
    let mut response = Html(html).into_response();
    response
        .headers_mut()
        .insert("content-security-policy", policy().parse().unwrap());
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response.extensions_mut().insert(SandboxedAppDocument);
    Ok(response)
}
fn policy() -> String {
    format!(
        "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; script-src-attr 'none'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'; object-src 'none'; frame-src 'none'; frame-ancestors 'self' {}",
        crate::runtime_config::get().frame_ancestors
    )
}
#[cfg(test)]
mod tests {
    #[test]
    fn document_policy_never_grants_same_origin_or_network() {
        let policy = super::policy();
        assert!(policy.starts_with("sandbox allow-scripts;"));
        assert!(!policy.contains("allow-same-origin"));
        for rule in [
            "connect-src 'none'",
            "form-action 'none'",
            "frame-src 'none'",
            "script-src-attr 'none'",
        ] {
            assert!(policy.contains(rule));
        }
    }
}
