//! One permission-aware action gateway serves HTTP, UI and MCP; service egress is operator configured.
use super::*;
pub(crate) async fn app_tools(a: &App, h: &RequestContext) -> Result<Vec<Value>> {
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
            if verified_kernel::app_tool_admissible(
                action.mcp != Some(false),
                action_authorized(a, h, action),
            ) {
                tools.push(json!({"name":format!("app.{}.{}",m.id,action.name),"description":action.description,"inputSchema":action.input_schema,"annotations":{"readOnlyHint":(action.read_only || ["list","configurations"].contains(&action.handler.as_str()))}}));
            }
        }
    }
    Ok(tools)
}
pub(crate) fn validate_input(schema: &Value, v: &Value) -> Result<()> {
    input_schema::validate(schema, v)
}
pub(crate) async fn invoke_app(
    a: &App,
    h: &RequestContext,
    id: &str,
    name: &str,
    v: &Value,
) -> Result<Value> {
    let start = std::time::Instant::now();
    let result = execute(a, h, id, name, v).await;
    let status = result
        .as_ref()
        .map(|_| StatusCode::OK)
        .unwrap_or_else(|e| e.0);
    observability::record(a, h, id, name, v, status, start.elapsed().as_millis()).await;
    result
}
async fn execute(a: &App, h: &RequestContext, id: &str, name: &str, v: &Value) -> Result<Value> {
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
    if !action.public
        && (["save", "service", "emit", "job"].contains(&action.handler.as_str())
            || action.permission.is_some())
    {
        auth::permit(
            h,
            action
                .permission
                .as_deref()
                .unwrap_or(if action.handler == "job" {
                    "apps.manage"
                } else {
                    "catalog"
                }),
        )?;
    }
    validate_input(&action.input_schema, v)?;
    if let Some(result) = super::hosted::action(a, &t, id, name).await? {
        return Ok(result);
    }
    let e = action
        .entity
        .as_ref()
        .and_then(|n| m.entities.iter().find(|e| e.name == *n));
    match action.handler.as_str() {
        "assets" | "asset_preview" => {
            auth::permit(h, "catalog.read")?;
            assets::app_file_callback(a, h, &action.handler, v).await
        }
        "asset_upload" => Err(bad(
            "Asset uploads require the bounded multipart surface endpoint",
        )),
        "list" => data::list_page(a, &t, &m, e.ok_or(bad("Entity required"))?, v).await,
        "save" => data::save(a, &t, &m, e.ok_or(bad("Entity required"))?, v).await,
        "configurations" => {
            merchant(a, h)?;
            configurations(a, &t, id).await
        }
        "payment_command" => crate::payments::app_command(a, h, id, v).await,
        "payment_onboarding" => crate::payments::onboarding(a, h, id, v).await,
        "service" => {
            if !m.permissions.contains(&"service.call".into()) || m.runtime != "service" {
                return Err(Error(
                    StatusCode::FORBIDDEN,
                    "Service permission required".into(),
                ));
            }
            let result = service_call(a, &t, id, &format!("actions/{name}"), v).await?;
            if result["purgeSources"] == true && m.permissions.contains(&"knowledge.write".into()) {
                purge_sources(a, &t, id, result["exportCursor"].as_i64().unwrap_or(0)).await?;
            }
            Ok(result)
        }
        "job" => jobs::enqueue(a, h, &m, action, v).await,
        "emit" => {
            if v.to_string().len() > 16384 {
                return Err(bad("App event exceeds limit"));
            }
            let event: i64 = sqlx::query_scalar(
                "INSERT INTO outbox(tenant,kind,data) VALUES($1,$2,$3) RETURNING id",
            )
            .bind(&t)
            .bind(format!("app.{id}.{name}"))
            .bind(v)
            .fetch_one(&a.db)
            .await?;
            Ok(json!({"eventId":event,"accepted":true}))
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
    service_call_pinned(a, t, id, path, v, None).await
}
pub(super) async fn service_call_pinned(
    a: &App,
    t: &str,
    id: &str,
    path: &str,
    v: &Value,
    digest: Option<&str>,
) -> Result<Value> {
    if crate::staging::parent(a, t).await?.is_some() {
        return Err(bad("External services are disabled in private sandboxes"));
    }
    // No URL or credential comes from the manifest, merchant, model or event payload.
    let _cluster =
        crate::performance::cluster_lease::Lease::acquire(a, t, &format!("app:{id}"), 8).await?;
    let _permit = a.app_limits.enter(t, id)?;
    if v.to_string().len() > 65536 {
        return Err(bad("App request exceeds limit"));
    }
    let configured = &crate::runtime_config::get().services;
    let config = &configured[id];
    let manifest = package(a, t, id, true).await?;
    if digest.is_some_and(|d| d != approval::canonical_digest(&manifest)) {
        return Err(conflict("App package changed before service dispatch"));
    }
    if !approval::approved(&manifest, config) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Package is not approved for this operator service".into(),
        ));
    }
    let url = config["url"].as_str().ok_or(Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "App service is not configured".into(),
    ))?;
    let parsed = reqwest::Url::parse(url).map_err(|_| bad("Invalid service URL"))?;
    if !service_policy::allowed(
        &parsed,
        &crate::runtime_config::get().private_service_origins,
    ) {
        return Err(bad(
            "Service requires HTTPS, loopback or an explicitly approved private origin",
        ));
    }
    let service_token = secrets::resolve(a, t, &manifest, "service")
        .await?
        .unwrap_or_else(|| config["token"].as_str().unwrap_or("").to_owned());
    let response = egress::client(&parsed)
        .await?
        .post(format!("{}/{path}", url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .bearer_auth(service_token)
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
    let configured = &crate::runtime_config::get().services;
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

/// MCP opt-out applies to discovery and direct invocation, without disabling the UI/HTTP action.
pub(crate) async fn invoke_mcp(
    a: &App,
    h: &RequestContext,
    id: &str,
    name: &str,
    v: &Value,
) -> Result<Value> {
    let m = package(a, &tenant(h)?, id, true).await?;
    if m.actions
        .iter()
        .find(|act| act.name == name)
        .is_none_or(|act| {
            !verified_kernel::app_tool_admissible(
                act.mcp != Some(false),
                action_authorized(a, h, act),
            )
        })
    {
        return Err(Error(
            StatusCode::NOT_FOUND,
            "App MCP tool not exposed".into(),
        ));
    }
    invoke_app(a, h, id, name, v).await
}

/// The registry uses the same live permission check as direct actions.
pub(super) fn action_authorized(a: &App, h: &RequestContext, action: &Action) -> bool {
    action.public
        || merchant(a, h).is_ok()
            && (!(["save", "service", "emit", "job"].contains(&action.handler.as_str())
                || action.permission.is_some())
                || auth::permit(
                    h,
                    action
                        .permission
                        .as_deref()
                        .unwrap_or(if action.handler == "job" {
                            "apps.manage"
                        } else {
                            "catalog"
                        }),
                )
                .is_ok())
}
