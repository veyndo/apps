//! Strict hook/read-only-record declarations; source kind is supplied by the trusted bounded executor, not metadata.
use crate::{Entity, Result, bad};
use serde::{Deserialize, Serialize};
pub struct Context<'a> {
    pub entities: &'a [Entity],
    pub permissions: &'a [String],
    pub commerce_hooks: Option<&'a Contract>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub source: String,
    pub hooks: Vec<String>,
    #[serde(default)]
    pub records: Vec<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub entity: String,
    pub id: String,
}
pub fn validate(m: &Context<'_>, component_source: impl Fn(&str) -> bool) -> Result<()> {
    if let Some(c) = &m.commerce_hooks
        && (!m.permissions.iter().any(|p| p == "commerce.hooks")
            || c.source.len() > 32768
            || !component_source(&c.source)
            || c.hooks.is_empty()
            || c.hooks.len() > 4
            || c.hooks
                .iter()
                .any(|h| !["price", "discount", "shipping", "validation"].contains(&h.as_str()))
            || c.hooks
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != c.hooks.len()
            || c.records.len() > 4
            || c.records.iter().any(|r| {
                r.id.is_empty()
                    || r.id.len() > 100
                    || !m.entities.iter().any(|e| e.name == r.entity)
            })
            || (!c.records.is_empty() && !m.permissions.iter().any(|p| p == "data.read")))
    {
        return Err(bad("Invalid typed commerce hook contract"));
    }
    Ok(())
}
