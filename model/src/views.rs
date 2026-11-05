//! Bounded native views and projections; validation is shared, runtime grants and execution remain host authority.
use crate::surfaces::Surface;
pub use crate::view_types::{ContextBinding, NativeBlock, NativeView};
use crate::{
    Action, Entity, Error, Result, bad, declarations::identifier, form_layout, ui_logic,
    view_binding,
};
use serde_json::{Value, json};
use std::collections::HashMap;
/// Borrowed projection of an admitted typed package; never a caller identity or runtime grant.
pub struct Contract<'a> {
    pub entities: &'a [Entity],
    pub actions: &'a [Action],
    pub surfaces: &'a [Surface],
    pub views: &'a [NativeView],
}
/// Required trusted host decisions use the same field policy and public-read predicate as runtime admission.
pub trait Policy: crate::fields::ValuePolicy {
    fn public_read(&self, writes: bool) -> bool;
}
pub fn is_native(m: &Contract<'_>, s: &Surface) -> bool {
    m.views
        .iter()
        .any(|v| s.ui_path == format!("native/{}", v.id))
}
pub fn validate<P: Policy>(m: &Contract<'_>, policy: &P) -> Result<()> {
    let mut views = std::collections::HashSet::new();
    if m.views.len() > 16 {
        return Err(bad("Native view limit/runtime invalid"));
    }
    for (view_index, v) in m.views.iter().enumerate() {
        let mut blocks = std::collections::HashSet::new();
        if !identifier(&v.id)
            || !views.insert(&v.id)
            || !["stack", "grid", "form"].contains(&v.layout.as_str())
            || v.blocks.len() > 32
        {
            return Err(bad("Invalid native view"));
        }
        for (block_index, b) in v.blocks.iter().enumerate() {
            form_layout::validate(b)?;
            if !identifier(&b.id)
                || !blocks.insert(&b.id)
                || !["text", "table", "cards", "form"].contains(&b.kind.as_str())
                    && !ui_logic::CONTROLS.contains(&b.kind.as_str())
                || b.title.is_empty()
                || b.title.values().any(|s| s.len() > 100)
                || b.text.values().any(|s| s.len() > 2000)
            {
                return Err(bad("Invalid native block"));
            }
            ui_logic::validate(m, v, b, policy)?;
            if ["text", "button", "frame", "tabs"].contains(&b.kind.as_str()) {
                if b.entity.is_some()
                    || b.read_action.is_some()
                    || b.write_action.is_some()
                    || b.context_binding.is_some()
                {
                    return Err(bad("Text cannot bind actions"));
                }
                continue;
            }
            let entity = b.entity.as_ref().ok_or(bad("Block needs entity"))?;
            if !m.entities.iter().any(|e| &e.name == entity) {
                return Err(bad("Unknown block entity"));
            }
            view_binding::validate(m, &v.id, b)?;
            for (name, handler, property) in [
                (&b.read_action, "list", "readAction"),
                (&b.write_action, "save", "writeAction"),
            ] {
                let pointer = format!("/views/{view_index}/blocks/{block_index}/{property}");
                if let Some(name) = name {
                    if !m.actions.iter().any(|a| {
                        &a.name == name && a.handler == handler && a.entity.as_ref() == Some(entity)
                    }) {
                        return Err(Error::validation(
                            &pointer,
                            "Invalid native block action binding",
                        ));
                    }
                } else if handler == "list" || b.kind == "form" || b.inline_edit {
                    return Err(Error::validation(&pointer, "Native block action required"));
                }
            }
            if b.inline_edit && b.kind != "table" {
                return Err(bad("Inline editing requires a data grid"));
            }
            if b.kind != "form" && !b.inline_edit && b.write_action.is_some() {
                return Err(bad("Read block cannot write"));
            }
            for s in m
                .surfaces
                .iter()
                .filter(|s| s.ui_path == format!("native/{}", v.id))
            {
                if !s.location.starts_with("admin.")
                    && !policy.public_read(b.kind == "form" || b.inline_edit)
                {
                    return Err(bad("Public native forms prohibited"));
                }
                if [&b.read_action, &b.write_action]
                    .into_iter()
                    .flatten()
                    .any(|n| !s.actions.contains(n))
                {
                    return Err(bad("Block action missing from surface allowlist"));
                }
            }
        }
    }
    for s in m.surfaces {
        if s.ui_path.starts_with("native/") && !is_native(m, s) {
            return Err(bad("Unknown native surface view"));
        }
    }
    Ok(())
}
pub fn payload(m: &Contract<'_>, s: &Surface) -> Option<Value> {
    if !is_native(m, s) {
        return None;
    }
    let v = m
        .views
        .iter()
        .find(|v| s.ui_path == format!("native/{}", v.id))?;
    let entities = m
        .entities
        .iter()
        .filter(|e| v.blocks.iter().any(|b| b.entity.as_ref() == Some(&e.name)))
        .collect::<Vec<_>>();
    Some(
        json!({"view":v,"entities":entities,"assetActions":m.actions.iter().filter(|a|["assets","asset_preview","asset_upload"].contains(&a.handler.as_str()) && s.actions.contains(&a.name)).map(|a|(&a.handler,&a.name)).collect::<HashMap<_,_>>(),"lookupActions":m.actions.iter().filter(|a|a.handler=="list" && s.actions.contains(&a.name)).filter_map(|a|a.entity.as_ref().map(|e|(e,&a.name))).collect::<HashMap<_,_>>(),"navigation":m.surfaces.iter().filter(|n|n.location.starts_with("admin.")==s.location.starts_with("admin.")).filter_map(|s|s.ui_path.strip_prefix("native/").map(|v|(v,&s.id))).collect::<HashMap<_,_>>()}),
    )
}
