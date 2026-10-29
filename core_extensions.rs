//! Namespaced core-object extension contracts; native records, rights and app storage keep their owners.
use super::{Entity, Manifest};
use crate::{RequestContext, Result, auth, bad, conflict};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Extension {
    pub version: u8,
    pub object: Object,
    pub field: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Object {
    Product,
    Customer,
    Order,
    Company,
}
impl Object {
    pub(crate) fn reference(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Customer => "customer",
            Self::Order => "order",
            Self::Company => "b2bCompany",
        }
    }
    pub(crate) fn scopes(self, write: bool) -> &'static [&'static str] {
        match (self, write) {
            (Self::Product, false) => &["catalog.read"],
            (Self::Product, true) => &["catalog.read", "catalog.write"],
            (Self::Customer, false) => &["customers.read", "customers.pii"],
            (Self::Customer, true) => &["customers.read", "customers.pii", "customers.write"],
            (Self::Order, false) => &["orders.read", "customers.pii"],
            (Self::Order, true) => &["orders.read", "customers.pii", "orders.write"],
            (Self::Company, false) => &["b2b.read"],
            (Self::Company, true) => &["b2b.read", "b2b.manage"],
        }
    }
}

pub(super) fn validate(m: &Manifest) -> Result<()> {
    for e in &m.entities {
        let Some(x) = &e.extends else { continue };
        let anchor = e
            .fields
            .iter()
            .find(|f| f.name == x.field)
            .ok_or(bad("Extension anchor field is missing"))?;
        if x.version != 1
            || e.public_read
            || e.publication.is_some()
            || !anchor.required
            || !anchor.indexed
            || !anchor.unique
            || anchor.kind != "string"
            || anchor.translatable
            || anchor.core_reference.as_deref() != Some(x.object.reference())
            || anchor.references.is_some()
        {
            return Err(bad(
                "Extensions need a private, required, indexed, unique native reference",
            ));
        }
        let actions: Vec<_> = m
            .actions
            .iter()
            .filter(|a| a.entity.as_deref() == Some(e.name.as_str()))
            .collect();
        if !m.permissions.iter().any(|p| p == "data.read")
            || !actions.iter().any(|a| a.handler == "list")
        {
            return Err(bad(
                "Extensions need a declared native list action and data.read",
            ));
        }
        for action in actions {
            if action.public || !matches!(action.handler.as_str(), "list" | "save") {
                return Err(bad(
                    "Extension records support private native list/save actions only",
                ));
            }
            let scopes = x.object.scopes(action.handler == "save");
            if scopes.iter().any(|scope| {
                action.permission.as_deref() != Some(*scope)
                    && !action.required_permissions.iter().any(|s| s == scope)
            }) {
                return Err(bad(
                    "Extension actions must require current native object read/write and PII rights",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn access(e: &Entity, h: &RequestContext, write: bool) -> Result<()> {
    if let Some(x) = &e.extends {
        for scope in x.object.scopes(write) {
            auth::permit(h, scope)?;
        }
    }
    Ok(())
}

pub(crate) fn configuration(e: &Entity) -> bool {
    let object = e.extends.as_ref().map(|x| x.object);
    crate::verified_kernel::app_configuration_release_admissible(
        e.extends.is_some(),
        object == Some(Object::Product),
        object == Some(Object::Company),
        e.operational == Some(true),
    )
}

pub(super) fn compatible(old: &Entity, next: &Entity) -> Result<()> {
    if old.operational == Some(true) && next.operational != Some(true) {
        return Err(conflict(
            "Operational records cannot become publishable configuration",
        ));
    }
    if old.extends != next.extends {
        return Err(conflict(
            "Changing an installed extension binding requires an explicit offline migration",
        ));
    }
    Ok(())
}

pub(crate) async fn descriptions(
    a: &crate::App,
    h: &RequestContext,
    object: Object,
) -> Result<serde_json::Value> {
    super::extension_projection::descriptions(a, h, object).await
}

/// Native lists and details share one bounded, permission-filtered app projection.
pub(crate) async fn attach(
    a: &crate::App,
    h: &RequestContext,
    object: Object,
    rows: &mut [serde_json::Value],
) -> Result<()> {
    super::extension_projection::attach(a, h, object, rows).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn package(object: Object) -> Manifest {
        serde_json::from_value(json!({
            "id":"native_notes","version":"1.0.0","coreApi":"1","runtime":"declarative",
            "name":{"en-GB":"Notes"},"permissions":["data.read","data.write"],
            "entities":[{"name":"notes","extends":{"version":1,"object":object,"field":"owner"},
                "fields":[{"name":"owner","kind":"string","required":true,"indexed":true,"unique":true,"coreReference":object.reference()}]}],
            "actions":[{"name":"read","description":"Read","handler":"list","entity":"notes","inputSchema":{"type":"object"},"requiredPermissions":object.scopes(false)},
                {"name":"write","description":"Write","handler":"save","entity":"notes","inputSchema":{"type":"object"},"requiredPermissions":object.scopes(true)}]
        })).unwrap()
    }
    #[test]
    fn native_bindings_need_native_rights_and_unique_private_anchors() {
        for object in [
            Object::Product,
            Object::Customer,
            Object::Order,
            Object::Company,
        ] {
            let m = package(object);
            assert!(validate(&m).is_ok());
            let mut unsafe_m = m.clone();
            unsafe_m.entities[0].fields[0].unique = false;
            assert!(validate(&unsafe_m).is_err());
            unsafe_m = m.clone();
            unsafe_m.entities[0].public_read = true;
            assert!(validate(&unsafe_m).is_err());
            unsafe_m = m.clone();
            unsafe_m.actions[0].required_permissions.clear();
            assert!(validate(&unsafe_m).is_err());
            unsafe_m = m.clone();
            unsafe_m.entities[0].extends = None;
            assert!(compatible(&m.entities[0], &unsafe_m.entities[0]).is_err());
        }
    }
    #[test]
    fn old_packages_keep_their_serialized_identity() {
        let mut m = package(Object::Product);
        m.entities[0].extends = None;
        assert!(
            !serde_json::to_value(&m).unwrap()["entities"][0]
                .as_object()
                .unwrap()
                .contains_key("extends")
        );
        assert!(
            serde_json::to_value(&m).unwrap()["entities"][0]
                .get("operational")
                .is_none()
        );
    }
    #[test]
    fn operational_receipts_do_not_become_release_configuration() {
        let mut m = package(Object::Product);
        let original = m.entities[0].clone();
        m.entities[0].operational = Some(true);
        assert!(!configuration(&m.entities[0]));
        assert!(compatible(&m.entities[0], &original).is_err());
        assert!(compatible(&original, &m.entities[0]).is_ok());
    }
}
