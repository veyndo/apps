//! One permission-aware action gateway serves HTTP, UI and MCP; service egress is operator configured.
use super::*;
pub(crate) async fn app_tools(a: &App, h: &HeaderMap) -> Result<Vec<Value>> {
    let rows =
        sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active ORDER BY id")
            .bind(tenant(h)?)
            .fetch_all(&a.db)
            .await?;
    let mut tools = vec![];
    for r in rows {
        let m: Manifest =
            serde_json::from_value(r.get("manifest")).map_err(|_| bad("Invalid package"))?;
        for action in &m.actions {
            if action.public
                || merchant(a, h).is_ok()
                    && (!["save", "service"].contains(&action.handler.as_str())
                        || auth::permit(h, "catalog").is_ok())
            {
                tools.push(json!({"name":format!("app.{}.{}",m.id,action.name),"description":action.description,"inputSchema":action.input_schema,"annotations":{"readOnlyHint":(["list","configurations"].contains(&action.handler.as_str()))}}));
            }
        }
    }
    Ok(tools)
}
fn validate_input(schema: &Value, v: &Value) -> Result<()> {
    let object = v.as_object().ok_or(bad("Action input must be an object"))?;
    let props = schema["properties"]
        .as_object()
        .ok_or(bad("Action needs properties"))?;
    if object.keys().any(|k| !props.contains_key(k)) {
        return Err(bad("Unknown action argument"));
    }
    if schema["required"].as_array().is_some_and(|keys| {
        keys.iter()
            .any(|k| k.as_str().is_none_or(|s| !object.contains_key(s)))
    }) {
        return Err(bad("Required action argument missing"));
    }
    for (key, value) in object {
        let kind = props[key]["type"].as_str().unwrap_or("");
        let ok = match kind {
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some(),
            "object" => value.is_object(),
            "boolean" => value.is_boolean(),
            _ => false,
        };
        if !ok {
            return Err(bad("Action argument type mismatch"));
        }
    }
    Ok(())
}
pub(crate) async fn invoke_app(
    a: &App,
    h: &HeaderMap,
    id: &str,
    name: &str,
    v: &Value,
) -> Result<Value> {
    let t = tenant(h)?;
    let m = package(a, &t, id, true).await?;
    let action = m
        .actions
        .iter()
        .find(|act| act.name == name)
        .ok_or(bad("Unknown app action"))?;
    if !action.public {
        merchant(a, h)?;
    }
    if action.handler == "save" || (action.handler == "service" && !action.public) {
        auth::permit(h, "catalog")?;
    }
    validate_input(&action.input_schema, v)?;
    let e = action
        .entity
        .as_ref()
        .and_then(|n| m.entities.iter().find(|e| e.name == *n));
    match action.handler.as_str() {
        "list" => data::list(a, &t, &m, e.ok_or(bad("Entity required"))?).await,
        "save" => data::save(a, &t, &m, e.ok_or(bad("Entity required"))?, v).await,
        "configurations" => {
            merchant(a, h)?;
            configurations(a, &t, id).await
        }
        "service" => {
            if !m.permissions.contains(&"service.call".into()) || m.runtime != "service" {
                return Err(Error(
                    StatusCode::FORBIDDEN,
                    "Service permission required".into(),
                ));
            }
            service_call(a, &t, id, &format!("actions/{name}"), v).await
        }
        _ => Err(bad("Unsupported app action")),
    }
}
pub(crate) async fn service_call(
    a: &App,
    t: &str,
    id: &str,
    path: &str,
    v: &Value,
) -> Result<Value> {
    if crate::staging::parent(a, t).await?.is_some() {
        return Err(bad("External services are disabled in private sandboxes"));
    }
    // No URL or credential comes from the manifest, merchant, model or event payload.
    let configured: Value = serde_json::from_str(&env::var("APP_SERVICES").unwrap_or("{}".into()))
        .map_err(|_| bad("Invalid operator service configuration"))?;
    let config = &configured[id];
    let url = config["url"].as_str().ok_or(Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "App service is not configured".into(),
    ))?;
    let parsed = reqwest::Url::parse(url).map_err(|_| bad("Invalid service URL"))?;
    if parsed.scheme() != "https"
        && !(parsed.scheme() == "http"
            && ["127.0.0.1", "localhost"].contains(&parsed.host_str().unwrap_or("")))
    {
        return Err(bad(
            "Service requires HTTPS or explicit loopback development URL",
        ));
    }
    let response = a
        .http
        .post(format!("{}/{path}", url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .bearer_auth(config["token"].as_str().unwrap_or(""))
        .header("x-tenant", t)
        .json(v)
        .send()
        .await
        .map_err(|_| Error(StatusCode::BAD_GATEWAY, "App service unavailable".into()))?;
    if !response.status().is_success() {
        return Err(Error(
            StatusCode::BAD_GATEWAY,
            "App service rejected request".into(),
        ));
    }
    if response.content_length().is_some_and(|n| n > 65536) {
        return Err(bad("App response exceeds limit"));
    }
    http_limits::json_body(response).await
}

pub(crate) fn ui_url(id: &str) -> Option<String> {
    let configured: Value = serde_json::from_str(&env::var("APP_SERVICES").ok()?).ok()?;
    let url = configured[id]["uiUrl"].as_str()?;
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.scheme() == "https"
        || parsed.scheme() == "http"
            && ["localhost", "127.0.0.1"].contains(&parsed.host_str().unwrap_or(""))
    {
        Some(url.into())
    } else {
        None
    }
}
