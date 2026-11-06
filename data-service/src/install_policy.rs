//! Complete compiler with explicit portable-data host policies; every unsupported native resource or effect is denied before installation.
use crate::{
    error::{Problem, Result},
    portable,
};
use axum::http::StatusCode;
use serde_json::Value;
use veyndo_app_storage::model::{self, Action, manifest::Manifest};
pub struct Policy<'a>(pub &'a [String]);
pub fn unsupported() -> Problem {
    Problem::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "install_policy_required",
        "This host installs private app_ data packages only; native resources, effects, UI, migrations and publisher trust require their actual host",
    )
}
impl model::fields::ValuePolicy for Policy<'_> {
    fn locale(&self, _: &str) -> bool {
        false
    }
    fn money(&self, _: &Value) -> bool {
        false
    }
    fn richtext(&self, _: &Value) -> bool {
        false
    }
    fn relations(&self, _: &Value) -> bool {
        false
    }
}
impl model::declarations::ReferencePolicy for Policy<'_> {
    fn admitted(&self, _: bool, _: bool) -> bool {
        false
    }
}
impl model::configuration::Policy for Policy<'_> {
    fn known_currency(&self, _: &str) -> bool {
        false
    }
}
impl model::package::Policy for Policy<'_> {
    fn known_scope(&self, s: &str) -> bool {
        self.0.iter().any(|v| v == s)
    }
    fn component_source(&self, _: &str) -> bool {
        false
    }
}
impl model::views::Policy for Policy<'_> {
    fn public_read(&self, _: bool) -> bool {
        false
    }
}
impl model::native_lifecycle::CompilerPolicy for Policy<'_> {
    fn scopes(&self) -> &[&str] {
        &[]
    }
    fn locale(&self, _: &str) -> bool {
        false
    }
    fn order_base(&self) -> model::native_lifecycle::Machine {
        model::native_lifecycle::Machine {
            states: vec![],
            transitions: vec![],
            source_app: None,
        }
    }
}
impl model::distribution::SignatureVerifier for Policy<'_> {
    type Error = Problem;
    fn verify_signature(&self, _: &Manifest) -> Result<()> {
        Err(unsupported())
    }
}
impl model::compiler::Policy for Policy<'_> {
    fn unsigned_namespace(&self, m: &Manifest) -> Result<()> {
        if m.id.starts_with("app_") {
            Ok(())
        } else {
            Err(unsupported())
        }
    }
    fn destination(&self, _: &str) -> Result<()> {
        Err(unsupported())
    }
    fn readonly(&self, a: &Action) -> bool {
        a.handler == "list" && !a.public
    }
    fn now(&self) -> model::compiler::DateTime<model::compiler::Utc> {
        model::compiler::Utc::now()
    }
}
pub fn compile(raw: Value, scopes: &[String]) -> Result<Manifest> {
    if raw.to_string().len() > 49152 {
        return Err(unsupported());
    }
    let m: Manifest = serde_json::from_value(raw).map_err(|_| {
        Problem::new(
            StatusCode::BAD_REQUEST,
            "invalid_package",
            "Use the complete strict App Manifest contract",
        )
    })?;
    if !m.id.starts_with("app_")
        || m.runtime != "declarative"
        || m.distribution.is_some()
        || m.configuration.is_some()
        || m.capability.is_some()
        || m.commerce_hooks.is_some()
        || m.payment_provider.is_some()
        || !m.lifecycles.is_empty()
        || !m.slots.is_empty()
        || !m.events.is_empty()
        || !m.event_filters.is_empty()
        || m.event_delivery.is_some()
        || !m.surfaces.is_empty()
        || !m.api_routes.is_empty()
        || !m.views.is_empty()
        || !m.schedules.is_empty()
        || !m.webhooks.is_empty()
        || !m.schema_migrations.is_empty()
        || m.permissions
            .iter()
            .any(|p| !matches!(p.as_str(), "data.read" | "data.write"))
        || m.actions
            .iter()
            .any(|a| a.public || !matches!(a.handler.as_str(), "list" | "save"))
    {
        return Err(unsupported());
    }
    for e in &m.entities {
        portable::check(&m, e)?;
    }
    model::compiler::compile(&m, &Policy(scopes)).map_err(|error| match error {
        model::package::CompileError::Host(error) => error,
        model::package::CompileError::Model(error) => {
            let mut problem = Problem::new(
                if error.kind == model::Kind::Conflict {
                    StatusCode::CONFLICT
                } else {
                    StatusCode::BAD_REQUEST
                },
                "invalid_package",
                error.message,
            );
            problem.pointer = error.pointer;
            problem
        }
    })?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_package_uses_the_complete_compiler() {
        let m: Manifest =
            serde_json::from_str(include_str!("../tests/portable-package.json")).unwrap();
        let scopes = vec!["catalog.read".into(), "catalog.write".into()];
        model::compiler::compile(&m, &Policy(&scopes)).unwrap();
    }
}
