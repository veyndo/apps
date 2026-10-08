//! Short-lived UI grants bind a package, surface, actor and concrete editor object on the server.
use super::*;
pub(super) fn router() -> Router<App> {
    Router::new()
        .secure_route(
            "/api/apps/{id}/surfaces/{surface}/grant",
            &[("POST", "read")],
            post(create),
        )
        .route(
            "/store-api/apps/{id}/surfaces/{surface}/grant",
            post(create),
        )
        .secure_route(
            "/api/apps/{id}/surfaces/{surface}/actions/{action}",
            &[("POST", "read")],
            post(invoke),
        )
        .route(
            "/store-api/apps/{id}/surfaces/{surface}/actions/{action}",
            post(invoke),
        )
}
pub(super) fn actor(h: &RequestContext) -> String {
    hash(&format!(
        "{}:{}",
        header(h, "authorization").unwrap_or(""),
        header(h, "sw-context-token").unwrap_or("")
    ))
}
pub(super) async fn context(a: &App, h: &RequestContext, v: &Value) -> Result<()> {
    let obj = v
        .as_object()
        .ok_or(bad("Surface context must be an object"))?;
    if obj.len() > 4 {
        return Err(bad("Surface context field limit"));
    }
    for (key, value) in obj {
        if key == "itemCount" {
            if value.as_u64().is_none_or(|n| n > 10000) {
                return Err(bad("Invalid cart count context"));
            }
            continue;
        }
        let (sql, scope) = match key.as_str() {
            "productId" => (
                "SELECT EXISTS(SELECT 1 FROM products WHERE tenant=$1 AND id=$2)",
                "catalog.read",
            ),
            "customerId" => (
                "SELECT EXISTS(SELECT 1 FROM customers WHERE tenant=$1 AND id=$2)",
                "customers.read",
            ),
            "orderId" => (
                "SELECT EXISTS(SELECT 1 FROM orders WHERE tenant=$1 AND id=$2)",
                "orders.read",
            ),
            "salesChannelId" | "salesChannel" => (
                "SELECT EXISTS(SELECT 1 FROM sales_channels WHERE tenant=$1 AND id=$2)",
                "catalog.read",
            ),
            _ => return Err(bad("Unknown surface context field")),
        };
        let id = value
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 100)
            .ok_or(bad("Invalid surface context reference"))?;
        if h.principal.role.is_none() && key == "productId" {
            marketing::admit_product(a, h, id).await?;
        } else if h.principal.role.is_none()
            && matches!(key.as_str(), "salesChannelId" | "salesChannel")
        {
            if id != marketing::channel_id(h) {
                return Err(bad("Surface channel differs from the current shop context"));
            }
            marketing::channel(a, h, id, &language_context(a, h).await?.0).await?;
        } else {
            auth::permit(h, scope)?;
        }
        if !sqlx::query_scalar::<_, bool>(sql)
            .bind(tenant(h)?)
            .bind(id)
            .fetch_one(&a.db)
            .await?
        {
            return Err(bad("Surface object does not belong to this shop"));
        }
    }
    Ok(())
}
async fn create(
    State(a): State<App>,
    h: RequestContext,
    Path((id, surface)): Path<(String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let t = tenant(&h)?;
    let m = package(&a, &t, &id, true).await?;
    let s = m
        .surfaces
        .iter()
        .find(|s| s.id == surface)
        .ok_or(bad("Unknown app surface"))?;
    if s.location.starts_with("admin.") {
        merchant(&a, &h)?;
    }
    if let Some(p) = &s.permission {
        auth::permit(&h, p)?;
    }
    let ctx = v.get("context").cloned().unwrap_or(json!({}));
    context(&a, &h, &ctx).await?;
    let actions = s
        .actions
        .iter()
        .filter(|name| {
            m.actions
                .iter()
                .any(|act| act.name == **name && gateway::action_authorized(&a, &h, act))
        })
        .collect::<Vec<_>>();
    let grant = format!("{}{}", uid(), uid());
    let subject = actor(&h);
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,812))")
        .bind(format!("{t}:{id}:{subject}"))
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM app_surface_grants WHERE tenant=$1 AND app=$2 AND expires_at<=now()")
        .bind(&t)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_surface_grants WHERE tenant=$1 AND app=$2 AND actor_hash=$3",
    )
    .bind(&t)
    .bind(&id)
    .bind(&subject)
    .fetch_one(&mut *tx)
    .await?;
    if count >= 100 {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many active app surfaces; retry after expiry".into(),
        ));
    }
    sqlx::query("INSERT INTO app_surface_grants(tenant,token_hash,app,surface,actor_hash,package_digest,actions,context,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,now()+interval '5 minutes')").bind(t).bind(hash(&grant)).bind(id).bind(surface).bind(subject).bind(approval::canonical_digest(&m)).bind(json!(actions)).bind(ctx).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"token":grant,"expiresIn":300,"actions":actions}),
    ))
}
pub(super) fn bind_input(
    m: &Manifest,
    s: &Surface,
    ctx: &Value,
    name: &str,
    v: &Value,
) -> Result<()> {
    for (key, id) in ctx
        .as_object()
        .ok_or(bad("Invalid stored surface context"))?
    {
        if v.get(key).is_some_and(|supplied| supplied != id) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "Action targets a different surface object".into(),
            ));
        }
    }
    for block in m
        .views
        .iter()
        .filter(|view| s.ui_path == format!("native/{}", view.id))
        .flat_map(|v| &v.blocks)
    {
        if let Some(binding) = &block.context_binding {
            let value = ctx
                .get(&binding.key)
                .ok_or(bad("Editor surface needs a context object"))?;
            if block.write_action.as_deref() == Some(name) && v["fields"][&binding.field] != *value
            {
                return Err(Error(
                    StatusCode::FORBIDDEN,
                    "Record must remain bound to the surface object".into(),
                ));
            }
            if block.read_action.as_deref() == Some(name) && v["filter"][&binding.field] != *value {
                return Err(Error(
                    StatusCode::FORBIDDEN,
                    "Read must filter by the surface object".into(),
                ));
            }
        }
    }
    Ok(())
}
async fn invoke(
    State(a): State<App>,
    mut h: RequestContext,
    Path((id, surface, name)): Path<(String, String, String)>,
    Json(v): Json<Value>,
) -> Result<Json<Value>> {
    let (m, s, ctx, actions) = load(&a, &h, &id, &surface, &v).await?;
    if !crate::verified_kernel::app_surface_admissible(
        true,
        actions.as_array().is_some_and(|a| a.contains(&json!(name))),
    ) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Action is outside the current surface grant".into(),
        ));
    }
    bind_input(&m, &s, &ctx, &name, &v["input"])?;
    if let Some(key) = v["requestKey"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 100)
    {
        h.insert(
            "idempotency-key",
            key.parse().map_err(|_| bad("Invalid request key"))?,
        );
    }
    Ok(Json(
        gateway::invoke_app(&a, &h, &id, &name, &v["input"]).await?,
    ))
}

/// Recheck current actor, immutable digest, role and object ownership for actions and UI bundle reads.
pub(super) async fn load(
    a: &App,
    h: &RequestContext,
    id: &str,
    surface: &str,
    v: &Value,
) -> Result<(Manifest, Surface, Value, Value)> {
    let t = tenant(h)?;
    let token = v["grant"].as_str().filter(|s| s.len() == 64).ok_or(Error(
        StatusCode::UNAUTHORIZED,
        "Surface grant required".into(),
    ))?;
    let row = sqlx::query("SELECT package_digest,actions,context FROM app_surface_grants WHERE tenant=$1 AND token_hash=$2 AND app=$3 AND surface=$4 AND actor_hash=$5 AND expires_at>now()")
        .bind(&t).bind(hash(token)).bind(id).bind(surface).bind(actor(h)).fetch_optional(&a.db).await?.ok_or(Error(StatusCode::UNAUTHORIZED,"Surface grant expired or belongs to another actor".into()))?;
    let m = package(a, &t, id, true).await?;
    if !crate::verified_kernel::app_surface_admissible(
        row.get::<String, _>("package_digest") == approval::canonical_digest(&m),
        true,
    ) {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "Surface package has changed".into(),
        ));
    }
    let s = m
        .surfaces
        .iter()
        .find(|s| s.id == surface)
        .cloned()
        .ok_or(bad("Surface removed"))?;
    if s.location.starts_with("admin.") {
        merchant(a, h)?;
    }
    if let Some(p) = &s.permission {
        auth::permit(h, p)?;
    }
    let ctx: Value = row.get("context");
    context(a, h, &ctx).await?;
    Ok((m, s, ctx, row.get("actions")))
}
