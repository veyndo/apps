//! Shared action/current-authority ports and callback consent rules; never authenticate from request fields or issue credentials.
use crate::{Action, Error, bad, schema};
use serde_json::Value;
#[derive(Debug)]
pub enum AdmissionError<E> {
    Authority(E),
    Model(Error),
}
/// Implemented only by the trusted host using its current authenticated principal and effective rights.
pub trait ScopeAuthority {
    type Error;
    fn require_scope(&self, scope: &str) -> Result<(), Self::Error>;
}
pub trait Authority: ScopeAuthority {
    fn require_merchant(&self) -> Result<(), Self::Error>;
}
pub fn additional<A: ScopeAuthority>(host: &A, action: &Action) -> Result<(), A::Error> {
    if action.handler == "configurations" {
        host.require_scope("orders.read")?;
    }
    for scope in &action.required_permissions {
        host.require_scope(scope)?;
    }
    Ok(())
}
pub fn check<A: Authority>(
    host: &A,
    action: &Action,
    value: &Value,
) -> Result<(), AdmissionError<A::Error>> {
    if !action.public {
        host.require_merchant().map_err(AdmissionError::Authority)?;
    }
    if !action.public
        && (["save", "service", "emit", "job"].contains(&action.handler.as_str())
            || action.permission.is_some())
    {
        let permission = action
            .permission
            .as_deref()
            .unwrap_or(if action.handler == "job" {
                "apps.manage"
            } else {
                "catalog"
            });
        host.require_scope(permission)
            .map_err(AdmissionError::Authority)?;
    }
    additional(host, action).map_err(AdmissionError::Authority)?;
    schema::validate(&action.input_schema, value).map_err(AdmissionError::Model)
}
pub fn callback_scope(permission: &str) -> Option<&'static str> {
    Some(match permission {
        "data.read" | "data.write" => "apps.manage",
        "b2b.manage" => "b2b.manage",
        "b2b.read" => "b2b.read",
        "orders.read" => "orders.read",
        "orders.import" | "deliveries.write" => "orders.write",
        "documents.create" => "documents.create",
        "payments.record" => "payments.manage",
        "channels.read" => "settings.read",
        "documents.read" => "documents.read",
        "customers.read" => "customers.read",
        "customers.write" => "customers.write",
        "settings.read" => "team.manage",
        "customers.pii" => "customers.pii",
        "products.read" => "catalog.read",
        "products.write" | "reviews.write" => "catalog.write",
        "promotions.write" => "automation.manage",
        "jobs.read" | "jobs.write" => "apps.manage",
        "assets.read" => "catalog.read",
        "assets.write" => "catalog.write",
        _ => return None,
    })
}
/// Facts for the host's existing verified callback predicate. is_app/grants come from the authenticated principal, not JSON/header claims.
pub fn callback_facts(
    is_app: bool,
    grants: Option<&[String]>,
    capability: &str,
    permitted: impl FnOnce(&str) -> bool,
) -> (bool, bool) {
    let consent = !is_app || grants.is_some_and(|grants| grants.iter().any(|p| p == capability));
    let role = callback_scope(capability).is_some_and(permitted);
    (consent, role)
}
/// Key creation still requires native personal/current-package/live-workspace checks and persistence outside this pure planner.
pub fn requested_scopes<A: ScopeAuthority>(
    host: &A,
    declared: &[String],
    requested: &Value,
) -> Result<Vec<&'static str>, AdmissionError<A::Error>> {
    let requested = requested
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or_else(|| AdmissionError::Model(bad("Explicit callback permissions required")))?;
    let mut scopes = Vec::new();
    for value in requested {
        let p = value
            .as_str()
            .ok_or_else(|| AdmissionError::Model(bad("Invalid callback permission")))?;
        let scope = callback_scope(p)
            .filter(|_| declared.iter().any(|v| v == p))
            .ok_or_else(|| {
                AdmissionError::Model(bad("Callback permission is not declared by this app"))
            })?;
        if p == "customers.pii"
            && !requested
                .iter()
                .any(|v| v == "customers.read" || v == "orders.read")
        {
            return Err(AdmissionError::Model(bad(
                "PII needs an explicit customer or order read permission",
            )));
        }
        host.require_scope(scope)
            .map_err(AdmissionError::Authority)?;
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    Ok(scopes)
}
