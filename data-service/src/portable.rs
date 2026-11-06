//! Explicit portable record policy; unsupported native resources never acquire permissive fallback validators.
use crate::error::{Problem, Result};
use axum::http::StatusCode;
use serde_json::Value;
use veyndo_app_storage::model::{self, Entity, manifest::Manifest};
pub fn check(m: &Manifest, e: &Entity) -> Result<()> {
    if m.runtime != "declarative"
        || m.id == "site_builder"
        || m.configuration.is_some()
        || m.capability.is_some()
        || m.commerce_hooks.is_some()
        || m.payment_provider.is_some()
        || !m.lifecycles.is_empty()
        || m.permissions
            .iter()
            .any(|p| !matches!(p.as_str(), "data.read" | "data.write"))
        || e.public_read
        || e.publication.is_some()
        || e.operational == Some(true)
        || e.extends.is_some()
        || e.fields.iter().any(|f| {
            f.translatable
                || f.indexed
                || f.unique
                || f.references.is_some()
                || f.core_reference.is_some()
                || !matches!(
                    f.kind.as_str(),
                    "string" | "integer" | "boolean" | "json" | "date" | "datetime" | "decimal"
                )
        })
    {
        return Err(Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "native_policy_required",
            "This model requires its native field, reference, index or commerce policy",
        ));
    }
    model::fields::contract(e).map_err(|_| {
        Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_model",
            "The installed record model is invalid",
        )
    })?;
    Ok(())
}
pub struct Policy;
impl model::fields::ValuePolicy for Policy {
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

#[cfg(test)]
mod tests {
    use super::*;
    use veyndo_app_storage::model::fields::ValuePolicy;
    fn manifest() -> Manifest {
        serde_json::from_value(serde_json::json!({"id":"notes","version":"0.1.0","coreApi":"1","runtime":"declarative","name":{"en":"Notes"},"permissions":["data.read","data.write"],"entities":[{"name":"notes","fields":[{"name":"title","kind":"string"}]}]})).unwrap()
    }
    #[test]
    fn unsupported_policies_cannot_be_replaced_by_default_admission() {
        let m = manifest();
        assert!(check(&m, &m.entities[0]).is_ok());
        for kind in ["image", "file", "money", "richtext", "relations", "unknown"] {
            let mut m = manifest();
            m.entities[0].fields[0].kind = kind.into();
            assert!(check(&m, &m.entities[0]).is_err());
        }
        assert!(!Policy.locale("en"));
        assert!(!Policy.money(&serde_json::json!({"minor":1})));
        assert!(!Policy.richtext(&serde_json::json!([])));
        assert!(!Policy.relations(&serde_json::json!([])));
    }
}
