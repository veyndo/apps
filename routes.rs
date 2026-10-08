//! Tenant-scoped package lifecycle, generated data endpoints and a shared action adapter.
use super::*;
pub(crate) fn app_router() -> Router<App> {
    Router::new()
        .merge(super::webhooks::router())
        .merge(evidence_routes::router())
        .merge(surfaces::router())
        .route("/store-api/apps/analytics.js", get(analytics_sdk))
        .secure_route(
            "/api/apps",
            &[("GET", "catalog.read"), ("POST", "apps.manage")],
            get(app_list).post(app_install),
        )
        .secure_route(
            "/api/apps/{id}",
            &[("PUT", "apps.manage")],
            axum::routing::put(app_state),
        )
        .secure_route(
            "/api/apps/{id}/actions/{action}",
            &[("POST", "read")],
            post(app_action),
        )
        .route("/store-api/apps/{id}/actions/{action}", post(app_action))
        .secure_route(
            "/api/apps/{id}/entities/{entity}",
            &[("GET", "catalog.read"), ("POST", "apps.manage")],
            get(entity_list).post(entity_save),
        )
        .route("/store-api/apps/slots", get(slots))
        .route("/store-api/apps/{id}/configure", post(configure_action))
}
async fn app_list(State(a): State<App>, h: RequestContext) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let (config, _) = commerce::config(&a, &t).await?;
    let connections = hosted::connections(&a, &t).await?;
    let rows=sqlx::query("SELECT id,version,manifest,active,revision,digest FROM app_packages WHERE tenant=$1 ORDER BY id").bind(t).fetch_all(&a.db).await?;
    let packages: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id: String = r.get("id");
            let connected: Vec<&Value> = connections.iter().filter(|f| f["appId"] == id).collect();
            json!({
                "id":id, "version":r.get::<String,_>("version"),
                "manifest":r.get::<Value,_>("manifest"), "uiUrl":gateway::ui_url(&id),
                "active":r.get::<bool,_>("active"), "revision":r.get::<i64,_>("revision"),
                "digest":r.get::<String,_>("digest"), "connections":connected,
                "managedBy":if connected.is_empty() { None } else { Some("experience") }
            })
        })
        .collect();
    Ok(Json(json!({
        "apiVersion":"1", "mainLocale":config.main_locale, "packages":packages,
        "builtIns":["engraving","paypal","shopware_payments","storyfront","google_analytics","gmail","slack","email"],
        "serviceExecution":"operator-configured external services; no in-process guest code"
    })))
}
async fn app_install(
    State(a): State<App>,
    h: RequestContext,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "users")?;
    let t = merchant(&a, &h)?;
    let text = match v["builtIn"].as_str() {
        Some("engraving") => Some(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        )),
        Some("storyfront") => Some(include_str!(
            "../../extensions/apps/storyfront/manifest.json"
        )),
        Some("paypal") => Some(include_str!("../../extensions/apps/paypal/manifest.json")),
        Some("shopware_payments") => Some(include_str!(
            "../../extensions/apps/shopware-payments/manifest.json"
        )),
        Some("google_analytics") => Some(include_str!(
            "../../extensions/apps/google-analytics/manifest.json"
        )),
        Some("gmail") => Some(include_str!("../../extensions/apps/gmail/manifest.json")),
        Some("email") => Some(include_str!("../../extensions/apps/email/manifest.json")),
        Some("slack") => Some(include_str!("../../extensions/apps/slack/manifest.json")),
        _ => None,
    };
    let m: Manifest = if let Some(s) = text {
        serde_json::from_str(s).map_err(|_| bad("Invalid built-in"))?
    } else {
        serde_json::from_value(v["manifest"].clone()).map_err(|e| bad(e.to_string()))?
    };
    if [
        "engraving",
        "paypal",
        "shopware_payments",
        "storyfront",
        "google_analytics",
        "gmail",
        "slack",
        "email",
    ]
    .contains(&m.id.as_str())
        && text.is_none()
        // The bundled discovery placeholder can be replaced by the separately deployed provider package.
        && !(m.id == "shopware_payments" && m.payment_provider.is_some())
    {
        return Err(bad("Built-in app IDs are reserved"));
    }
    let result = install(&a, &t, m).await?;
    if v["builtIn"] == "paypal" {
        let mut tx = a.db.begin().await?;
        let row = sqlx::query("SELECT data FROM commerce_settings WHERE tenant=$1 FOR UPDATE")
            .bind(&t)
            .fetch_one(&mut *tx)
            .await?;
        let mut config: commerce::Settings =
            serde_json::from_value(row.get("data")).map_err(|_| bad("Invalid settings"))?;
        let live = payments::environment() == "live";
        let method = if live {
            "paypal-live"
        } else {
            "paypal-sandbox"
        };
        if !config.payments.iter().any(|p| p.id == method) {
            config.payments.push(commerce::Payment {
                id: method.into(),
                name: if live { "PayPal" } else { "PayPal Sandbox" }.into(),
                active: payments::account(&t).is_ok(),
                business_only: false,
                translations: HashMap::new(),
                countries: Vec::new(),
                restricted_countries: false,
                mode: "app".into(),
                provider: None,
                provider_method: None,
            });
            sqlx::query("UPDATE commerce_settings SET data=$1,revision=revision+1 WHERE tenant=$2")
                .bind(json!(config))
                .bind(&t)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
    }
    Ok(Json(result))
}
async fn app_state(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "users")?;
    let t = merchant(&a, &h)?;
    let active = v["active"].as_bool().ok_or(bad("active required"))?;
    let mut tx = a.db.begin().await?;
    crate::marketing::lock_config(&mut tx, &t).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,7))")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if !active
        && sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM hosted_frontends WHERE tenant=$1 AND app_id=$2)",
        )
        .bind(&t)
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?
    {
        return Err(conflict(
            "App is required by connected frontends; disconnect them first",
        ));
    }
    let changed=sqlx::query("UPDATE app_packages SET active=$1,revision=revision+1 WHERE tenant=$2 AND id=$3 AND revision=$4").bind(active).bind(t).bind(id).bind(v["revision"].as_i64().ok_or(bad("revision required"))?).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(conflict("App revision changed"));
    }
    tx.commit().await?;
    Ok(Json(json!({"active":active,"dataRetained":true})))
}
async fn app_action(
    State(a): State<App>,
    h: RequestContext,
    Path((id, action)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(invoke_app(&a, &h, &id, &action, &v).await?))
}
async fn entity_list(
    State(a): State<App>,
    h: RequestContext,
    Path((id, name)): Path<(String, String)>,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Result<Json<Value>> {
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    let e = m
        .entities
        .iter()
        .find(|e| e.name == name)
        .ok_or(bad("Unknown entity"))?;
    super::editor_contract::entity_access(&m, e, &h, "list")?;
    let input = json!({"limit":q.get("limit").map(|s| s.parse::<i64>()).transpose().map_err(|_| bad("Invalid app limit"))?,"after":q.get("after"),"filter":q.get("filter").map(|s| serde_json::from_str::<Value>(s)).transpose().map_err(|_| bad("Invalid app filter"))?});
    let mut input = input;
    if input["filter"].is_null() {
        input.as_object_mut().unwrap().remove("filter");
    }
    Ok(Json(data::list_page(&a, &t, &m, e, &input).await?))
}
async fn entity_save(
    State(a): State<App>,
    h: RequestContext,
    Path((id, name)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    auth::permit(&h, "catalog")?;
    let t = merchant(&a, &h)?;
    let m = package(&a, &t, &id, true).await?;
    let e = m
        .entities
        .iter()
        .find(|e| e.name == name)
        .ok_or(bad("Unknown entity"))?;
    super::editor_contract::entity_access(&m, e, &h, "save")?;
    Ok(Json(data::save(&a, &t, &m, e, &v).await?))
}
async fn slots(State(a): State<App>, h: RequestContext) -> Result<Json<Value>> {
    let rows = sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active")
        .bind(tenant(&h)?)
        .fetch_all(&a.db)
        .await?;
    let mut slots = vec![];
    for r in rows {
        let m: Manifest =
            serde_json::from_value(r.get("manifest")).map_err(|_| bad("Invalid package"))?;
        if m.permissions.contains(&"storefront.slot".into()) {
            for s in m.slots.iter().filter(|s| s.location == "product.detail") {
                slots.push(json!({"app":m.id,"version":m.version,"slot":s,"entities":m.entities.iter().filter(|e|e.public_read).map(|e|json!({"name":e.name,"label":e.label,"fields":e.fields,"action":m.actions.iter().find(|a|a.public && a.handler=="list" && a.entity.as_deref()==Some(e.name.as_str())).map(|a|&a.name)})).collect::<Vec<_>>(),"configuration":m.configuration.as_ref().map(|c|json!({"inputField":c.input_field,"label":c.label,"hint":c.hint}))}));
            }
        }
    }
    Ok(Json(json!({"slots":slots})))
}
async fn configure_action(
    State(a): State<App>,
    h: RequestContext,
    Path(id): Path<String>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    Ok(Json(configure(&a, &h, &id, &v).await?))
}

/// The shared public consent adapter contains no tenant configuration or provider credentials.
async fn analytics_sdk() -> impl axum::response::IntoResponse {
    (
        [
            ("content-type", "text/javascript; charset=utf-8"),
            ("cache-control", "public, max-age=3600"),
            ("x-content-type-options", "nosniff"),
        ],
        include_str!("../../extensions/sdk/analytics.js"),
    )
}
