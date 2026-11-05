//! Strict app placement and named HTTP route contracts; view, team-scope and effect authority are mandatory trusted host policies.
use crate::{Action, Error, Result, bad, declarations::identifier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
/// A projection from the complete typed package; it does not replace its other validators.
pub struct Contract<'a> {
    pub runtime: &'a str,
    pub permissions: &'a [String],
    pub actions: &'a [Action],
    pub surfaces: &'a [Surface],
    pub api_routes: &'a [ApiRoute],
}
/// No default approvals. Implement with authoritative host registries, views and effect rules.
pub trait Policy {
    fn native_view(&self, surface: &Surface) -> bool;
    fn known_scope(&self, scope: &str) -> bool;
    fn readonly(&self, action: &Action) -> bool;
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Surface {
    pub id: String,
    pub location: String,
    pub label: HashMap<String, String>,
    pub ui_path: String,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApiRoute {
    pub path: String,
    pub method: String,
    pub scope: String,
    pub action: String,
}
pub fn validate<P: Policy>(m: &Contract<'_>, policy: &P) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if m.surfaces.len() > 16 || m.api_routes.len() > 24 {
        return Err(bad("App surface/route limit exceeded"));
    }
    for (surface_index, s) in m.surfaces.iter().enumerate() {
        let private = s.location.starts_with("admin.");
        for (action_index, name) in s.actions.iter().enumerate() {
            if !m
                .actions
                .iter()
                .any(|a| a.name == *name && (private || a.public))
            {
                return Err(Error::validation(
                    &format!("/surfaces/{surface_index}/actions/{action_index}"),
                    format!(
                        "Surface {} refers to unavailable action {name}; use a declared action allowed for this surface",
                        s.id
                    ),
                ));
            }
        }
        if (m.runtime != "service" || s.location == "storefront.block") && !policy.native_view(s) {
            return Err(Error::validation(
                &format!("/surfaces/{surface_index}/uiPath"),
                "Native surface uiPath must be native/ followed by an existing view ID".to_owned(),
            ));
        }
        let slot_permission = if private {
            "admin.slot"
        } else {
            "storefront.slot"
        };
        if !m.permissions.iter().any(|p| p == slot_permission) {
            return Err(Error::validation(
                "/permissions",
                format!(
                    "Surface {} requires declared permission {slot_permission}",
                    s.id
                ),
            ));
        }
        if s.permission
            .as_deref()
            .is_some_and(|p| !private || !policy.known_scope(p))
        {
            return Err(Error::validation(&format!("/surfaces/{surface_index}/permission"), "Surface permission must be a current team scope for private admin surfaces or null for public surfaces".to_owned()));
        }
        if !identifier(&s.id)
            || !ids.insert(&s.id)
            || ![
                "admin.navigation",
                "admin.b2b.company",
                "admin.b2b.unit",
                "account.b2b",
                "admin.pos.catalog",
                "admin.pos.cart",
                "admin.pos.receipt",
                "admin.order",
                "admin.product",
                "admin.product.general",
                "admin.product.tab",
                "admin.customer",
                "admin.order.general",
                "storefront.page",
                "storefront.block",
                "storefront.home",
                "storefront.header",
                "product.detail",
                "cart.summary",
                "checkout.content",
                "checkout.summary",
                "account.overview",
            ]
            .contains(&s.location.as_str())
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
        {
            return Err(Error::validation(
                &format!("/surfaces/{surface_index}"),
                "Invalid app UI surface or exposed action".to_owned(),
            ));
        }
    }
    let mut routes = std::collections::HashSet::new();
    for r in m.api_routes {
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
            || (r.method == "GET" && !policy.readonly(action))
        {
            return Err(bad(
                "Invalid app HTTP route; GET requires a read-only action",
            ));
        }
    }
    Ok(())
}
