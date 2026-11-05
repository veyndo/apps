//! Native-object extension declaration rules; actual record identity, current scopes and reference ownership stay with the host.
use crate::{Action, Entity, Result, bad};
pub fn validate(entities: &[Entity], actions: &[Action], permissions: &[String]) -> Result<()> {
    for e in entities {
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
        let actions: Vec<_> = actions
            .iter()
            .filter(|a| a.entity.as_deref() == Some(e.name.as_str()))
            .collect();
        if !permissions.iter().any(|p| p == "data.read")
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
