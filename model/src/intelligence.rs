//! Strict app AI tool/model declarations and a borrowed projection from the complete admitted host package.
use crate::{Action, Entity, Result, bad};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub description: HashMap<String, String>,
    pub tools: Vec<String>,
    pub entities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology: Vec<crate::ontology::Mapping>,
}
/// Metadata only; this context neither authenticates an actor nor reads any record.
pub struct Context<'a> {
    pub id: &'a str,
    pub version: &'a str,
    pub entities: &'a [Entity],
    pub actions: &'a [Action],
    pub permissions: &'a [String],
    pub intelligence: Option<&'a Contract>,
}
pub fn validate(
    intelligence: Option<&Contract>,
    actions: &[Action],
    entities: &[Entity],
) -> Result<()> {
    if let Some(ai) = intelligence
        && (ai.tools.len() > 24
            || ai.entities.len() > 12
            || ai.description.values().any(|s| s.len() > 1000)
            || ai
                .tools
                .iter()
                .any(|n| !actions.iter().any(|a| a.name == *n))
            || ai
                .entities
                .iter()
                .any(|n| !entities.iter().any(|e| e.name == *n)))
    {
        return Err(bad("Invalid app intelligence contract"));
    }
    Ok(())
}
