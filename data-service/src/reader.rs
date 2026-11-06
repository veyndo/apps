//! Current private action admission and one signed read snapshot; no model installation, object-resolution bypass or data writes.
use crate::{
    App,
    contract::Read,
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
};
use axum::http::StatusCode;
use serde_json::Value;
use veyndo_app_storage::{
    model::{
        self,
        admission::{Authority, ScopeAuthority},
        manifest::Manifest,
    },
    record_query,
};
fn denied() -> Problem {
    Problem::new(
        StatusCode::FORBIDDEN,
        "permission_denied",
        "Current authority does not allow this App action",
    )
}
impl ScopeAuthority for Principal {
    type Error = Problem;
    fn require_scope(&self, s: &str) -> Result<()> {
        if self.permits(s) {
            Ok(())
        } else {
            Err(denied())
        }
    }
}
impl Authority for Principal {
    fn require_merchant(&self) -> Result<()> {
        if self.merchant { Ok(()) } else { Err(denied()) }
    }
}
pub async fn read(app: &App, p: Principal, input: Read, mcp: bool) -> Result<Value> {
    let bad = || {
        Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid_query",
            "Provide a bounded App record query matching the installed list action",
        )
    };
    if [&input.app, &input.action]
        .iter()
        .any(|s| s.is_empty() || !veyndo_app_authority::database::identifier(s))
    {
        return Err(bad());
    }
    let value = input.query.value();
    model::schema::validate(
        &crate::contract::schema(),
        &serde_json::json!({"app":input.app,"action":input.action,"query":value}),
    )
    .map_err(|_| bad())?;
    let _slot = app.execution.clone().try_acquire_owned().map_err(|_| {
        Problem::new(
            StatusCode::TOO_MANY_REQUESTS,
            "data_capacity",
            "The data service is busy; retry shortly",
        )
    })?;
    let mut tx = app.begin(&p).await?;
    let raw: Option<Value> = sqlx::query_scalar(
        "SELECT manifest FROM app_packages WHERE tenant=$1 AND id=$2 AND active",
    )
    .bind(&p.tenant)
    .bind(&input.app)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unavailable())?;
    let raw = raw.ok_or(Problem::new(
        StatusCode::NOT_FOUND,
        "app_unavailable",
        "The installed App action is unavailable",
    ))?;
    let digest = veyndo_app_storage::packages::digest(&raw);
    let manifest: Manifest = serde_json::from_value(raw).map_err(|_| unavailable())?;
    if manifest.id != input.app {
        return Err(unavailable());
    }
    let action = manifest
        .actions
        .iter()
        .find(|a| a.name == input.action && a.handler == "list" && !a.public)
        .ok_or(Problem::new(
            StatusCode::NOT_FOUND,
            "action_unavailable",
            "The private installed list action is unavailable",
        ))?;
    if mcp && action.mcp != Some(true) {
        return Err(Problem::new(
            StatusCode::FORBIDDEN,
            "mcp_denied",
            "This App action is not enabled for MCP",
        ));
    }
    model::admission::check(&p, action, &value).map_err(|e| match e {
        model::admission::AdmissionError::Authority(e) => e,
        model::admission::AdmissionError::Model(_) => bad(),
    })?;
    let entity = manifest
        .entities
        .iter()
        .find(|e| Some(&e.name) == action.entity.as_ref())
        .ok_or_else(unavailable)?;
    if entity.extends.is_some() || entity.fields.iter().any(|f| f.core_reference.is_some()) {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "native_resolver_required",
            "This model requires its native object and privacy resolver",
        ));
    }
    if !manifest.permissions.iter().any(|s| s == "data.read") {
        return Err(denied());
    }
    let plan =
        record_query::prepare(&manifest.permissions, entity, &value, None).map_err(|_| bad())?;
    let page = veyndo_app_storage::queries::list(
        &mut tx,
        veyndo_app_storage::Scope {
            tenant: &p.tenant,
            app: &input.app,
        },
        &entity.name,
        &plan.query(),
    )
    .await
    .map_err(|_| unavailable())?;
    tx.commit().await.map_err(|_| unavailable())?;
    let mut result = record_query::response(entity, plan.limit, page);
    let context = model::intelligence::Context {
        id: &manifest.id,
        version: &manifest.version,
        entities: &manifest.entities,
        actions: &manifest.actions,
        permissions: &manifest.permissions,
        intelligence: manifest.intelligence.as_ref(),
    };
    if let Some(graph) = model::ontology::project(&p.tenant, &context, entity, &result["elements"])
    {
        result["ontology"] = graph;
    }
    if app.config.writes {
        result["packageDigest"] = serde_json::json!(digest);
    }
    Ok(result)
}
