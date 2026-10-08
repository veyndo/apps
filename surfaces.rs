//! App-owned UI surfaces and namespaced HTTP routes reuse the authorized action gateway.
use super::*;
use axum::extract::{OriginalUri, Query};
pub(crate) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/surfaces",
            &[("GET", "catalog.read")],
            get(admin_surfaces),
        )
        .route("/store-api/apps/surfaces", get(public_surfaces))
        .secure_route(
            "/api/apps/{id}/http/{route}",
            &[("GET", "read"), ("POST", "read")],
            get(read_route).post(write_route),
        )
        .route(
            "/store-api/apps/{id}/http/{route}",
            get(read_route).post(write_route),
        )
        .layer(axum::middleware::from_fn(no_store))
}
async fn no_store(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "cache-control",
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}
pub(crate) fn readonly(action: &Action) -> bool {
    verified_kernel::app_read_admissible(
        action.read_only || ["list", "configurations"].contains(&action.handler.as_str()),
        ["save", "emit"].contains(&action.handler.as_str()),
    )
}
pub(crate) fn validate_contract(m: &Manifest) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if m.surfaces.len() > 16 || m.api_routes.len() > 24 {
        return Err(bad("App surface/route limit exceeded"));
    }
    for s in &m.surfaces {
        let private = s.location.starts_with("admin.");
        if !identifier(&s.id)
            || !ids.insert(&s.id)
            || ![
                "admin.navigation",
                "admin.order",
                "admin.product",
                "admin.product.general",
                "admin.product.tab",
                "admin.customer",
                "admin.order.general",
                "storefront.page",
                "storefront.home",
                "storefront.header",
                "product.detail",
                "cart.summary",
                "account.overview",
            ]
            .contains(&s.location.as_str())
            || (m.runtime != "service" && !native_views::is_native(m, s))
            || !m.permissions.iter().any(|p| {
                p == if private {
                    "admin.slot"
                } else {
                    "storefront.slot"
                }
            })
            || s.ui_path.is_empty()
            || s.ui_path.len() > 160
            || s.ui_path.split('/').any(|p| {
                p.is_empty()
                    || p == "."
                    || p == ".."
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
            })
            || s.actions.len() > 24
            || s.actions.iter().any(|n| {
                !m.actions
                    .iter()
                    .any(|a| a.name == *n && (private || a.public))
            })
            || s.permission
                .as_deref()
                .is_some_and(|p| !private || !auth::SCOPES.contains(&p))
        {
            return Err(bad("Invalid app UI surface or exposed action"));
        }
    }
    let mut routes = std::collections::HashSet::new();
    for r in &m.api_routes {
        let action = m
            .actions
            .iter()
            .find(|a| a.name == r.action)
            .ok_or(bad("Unknown route action"))?;
        if !identifier(&r.path)
            || !routes.insert((&r.scope, &r.method, &r.path))
            || !["admin", "storefront"].contains(&r.scope.as_str())
            || !["GET", "POST"].contains(&r.method.as_str())
            || (r.scope == "storefront" && !action.public)
            || (r.method == "GET" && !readonly(action))
        {
            return Err(bad(
                "Invalid app HTTP route; GET requires a read-only action",
            ));
        }
    }
    if let Some(ai) = &m.intelligence
        && (ai.tools.len() > 24
            || ai.entities.len() > 12
            || ai.description.values().any(|s| s.len() > 1000)
            || ai
                .tools
                .iter()
                .any(|n| !m.actions.iter().any(|a| a.name == *n))
            || ai
                .entities
                .iter()
                .any(|n| !m.entities.iter().any(|e| e.name == *n)))
    {
        return Err(bad("Invalid app intelligence contract"));
    }
    Ok(())
}
pub(super) fn surface_url(id: &str, path: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(&gateway::ui_url(id)?).ok()?;
    let base = url.path().trim_end_matches('/');
    let next = format!("{base}/{path}");
    url.set_path(&next);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}
async fn registry(a: &App, h: &RequestContext, public: bool) -> Result<Value> {
    if !public {
        merchant(a, h)?;
    }
    let (settings, _) = commerce::config(a, &tenant(h)?).await?;
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id")
            .bind(tenant(h)?)
            .fetch_all(&a.db)
            .await?;
    let mut surfaces = vec![];
    for row in rows {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid app package"))?;
        for s in &m.surfaces {
            if s.location.starts_with("admin.") == public
                || s.permission
                    .as_deref()
                    .is_some_and(|p| auth::permit(h, p).is_err())
            {
                continue;
            }
            if let Some(mut view) = native_views::payload(&m, s) {
                let allowed = s
                    .actions
                    .iter()
                    .filter(|name| {
                        m.actions
                            .iter()
                            .any(|act| act.name == **name && gateway::action_authorized(a, h, act))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if let Some(blocks) = view["view"]["blocks"].as_array_mut() {
                    blocks.retain(|b| {
                        ["text", "button", "frame", "tabs"]
                            .iter()
                            .any(|kind| b["kind"] == *kind)
                            || b["readAction"]
                                .as_str()
                                .is_some_and(|n| allowed.iter().any(|a| a == n))
                    });
                    for b in blocks {
                        if b["kind"] == "form"
                            && b["writeAction"]
                                .as_str()
                                .is_none_or(|n| !allowed.iter().any(|a| a == n))
                        {
                            b["kind"] = json!("cards");
                            b.as_object_mut().unwrap().remove("writeAction");
                        }
                    }
                }
                view["tenant"] = json!(tenant(h)?);
                let mut surface = json!(s);
                surface["actions"] = json!(allowed);
                surfaces.push(json!({"app":m.id,"version":m.version,"surface":surface,"native":view,"mainLocale":settings.main_locale,"locales":settings.locales}));
            } else if approval::approved(&m, &crate::runtime_config::get().services[&m.id])
                && let Some(url) = surface_url(&m.id, &s.ui_path)
            {
                surfaces.push(json!({"app":m.id,"version":m.version,"surface":s,"url":url}));
            }
        }
    }
    Ok(json!({"surfaces":surfaces}))
}
async fn admin_surfaces(State(a): State<App>, h: RequestContext) -> Result<Json<Value>> {
    Ok(Json(registry(&a, &h, false).await?))
}
async fn public_surfaces(State(a): State<App>, h: RequestContext) -> Result<Json<Value>> {
    Ok(Json(registry(&a, &h, true).await?))
}
async fn dispatch(
    a: &App,
    h: &RequestContext,
    id: &str,
    route: &str,
    method: &str,
    public: bool,
    v: &Value,
) -> Result<Value> {
    let m = package(a, &tenant(h)?, id, true).await?;
    let scope = if public { "storefront" } else { "admin" };
    let route = m
        .api_routes
        .iter()
        .find(|r| r.path == route && r.method == method && r.scope == scope)
        .ok_or(Error(
            StatusCode::NOT_FOUND,
            "App route not registered".into(),
        ))?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == route.action)
        .ok_or(bad("Unknown app action"))?;
    if method == "GET" && !readonly(action) {
        return Err(bad("Read-only app route required"));
    }
    invoke_app(a, h, id, &route.action, v).await
}
async fn read_route(
    State(a): State<App>,
    h: RequestContext,
    OriginalUri(uri): OriginalUri,
    Path((id, route)): Path<(String, String)>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let m = package(&a, &tenant(&h)?, &id, true).await?;
    let scope = if uri.path().starts_with("/store-api/") {
        "storefront"
    } else {
        "admin"
    };
    let r = m
        .api_routes
        .iter()
        .find(|r| r.path == route && r.method == "GET" && r.scope == scope)
        .ok_or(Error(
            StatusCode::NOT_FOUND,
            "App route not registered".into(),
        ))?;
    let action = m
        .actions
        .iter()
        .find(|a| a.name == r.action)
        .ok_or(bad("Unknown app action"))?;
    let mut input = serde_json::Map::new();
    for (key, value) in query {
        let v = if action.input_schema["properties"][&key]["type"] == "string" {
            json!(value)
        } else {
            serde_json::from_str(&value).map_err(|_| bad("Invalid typed query argument"))?
        };
        input.insert(key, v);
    }
    Ok(Json(
        dispatch(
            &a,
            &h,
            &id,
            &route,
            "GET",
            scope == "storefront",
            &Value::Object(input),
        )
        .await?,
    ))
}
async fn write_route(
    State(a): State<App>,
    h: RequestContext,
    OriginalUri(uri): OriginalUri,
    Path((id, route)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(
        dispatch(
            &a,
            &h,
            &id,
            &route,
            "POST",
            uri.path().starts_with("/store-api/"),
            &v,
        )
        .await?,
    ))
}
