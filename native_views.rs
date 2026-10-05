//! Bounded native view definitions; every data binding resolves to the same authorized app action gateway.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeView {
    pub id: String,
    pub layout: String,
    pub blocks: Vec<NativeBlock>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeBlock {
    pub id: String,
    pub kind: String,
    pub title: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub text: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_binding: Option<ContextBinding>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ContextBinding {
    pub field: String,
    pub key: String,
}
pub(super) fn is_native(m: &Manifest, s: &Surface) -> bool {
    m.views
        .iter()
        .any(|v| s.ui_path == format!("native/{}", v.id))
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let mut views = std::collections::HashSet::new();
    if m.views.len() > 16 {
        return Err(bad("Native view limit/runtime invalid"));
    }
    for v in &m.views {
        let mut blocks = std::collections::HashSet::new();
        if !identifier(&v.id)
            || !views.insert(&v.id)
            || !["stack", "grid"].contains(&v.layout.as_str())
            || v.blocks.len() > 32
        {
            return Err(bad("Invalid native view"));
        }
        for b in &v.blocks {
            if !identifier(&b.id)
                || !blocks.insert(&b.id)
                || !["text", "table", "cards", "form"].contains(&b.kind.as_str())
                || b.title.is_empty()
                || b.title.values().any(|s| s.len() > 100)
                || b.text.values().any(|s| s.len() > 2000)
            {
                return Err(bad("Invalid native block"));
            }
            if b.kind == "text" {
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
            super::editor_contract::validate_binding(m, &v.id, b)?;
            for (name, handler) in [(&b.read_action, "list"), (&b.write_action, "save")] {
                if let Some(name) = name {
                    if !m.actions.iter().any(|a| {
                        &a.name == name && a.handler == handler && a.entity.as_ref() == Some(entity)
                    }) {
                        return Err(bad("Invalid native block action binding"));
                    }
                } else if handler == "list" || b.kind == "form" {
                    return Err(bad("Native block action required"));
                }
            }
            if b.kind != "form" && b.write_action.is_some() {
                return Err(bad("Read block cannot write"));
            }
            for s in m
                .surfaces
                .iter()
                .filter(|s| s.ui_path == format!("native/{}", v.id))
            {
                if !s.location.starts_with("admin.")
                    && !verified_kernel::app_read_admissible(true, b.kind == "form")
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
    for s in &m.surfaces {
        if s.ui_path.starts_with("native/") && !is_native(m, s) {
            return Err(bad("Unknown native surface view"));
        }
    }
    Ok(())
}
pub(super) fn payload(m: &Manifest, s: &Surface) -> Option<Value> {
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
    Some(json!({"view":v,"entities":entities}))
}
