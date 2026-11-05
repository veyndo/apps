//! Test-only owned context for standalone graph mapping and declaration checks, with no identity or record resolver.
use serde_json::json;
use veyndo_app_model::{
    Action, Entity,
    intelligence::{Context, Contract},
};
pub struct Fixture {
    pub entities: Vec<Entity>,
    pub actions: Vec<Action>,
    pub permissions: Vec<String>,
    pub ai: Contract,
}
impl Fixture {
    pub fn new() -> Self {
        Self {
  entities:serde_json::from_value(json!([{"name":"guides","fields":[{"name":"product_id","kind":"string","coreReference":"product","indexed":true,"required":true},{"name":"care","kind":"string"},{"name":"internal","kind":"string"}]}])).unwrap(),
  actions:serde_json::from_value(json!([{"name":"list_guides","description":"Read","handler":"list","entity":"guides","mcp":false,"inputSchema":{"type":"object","additionalProperties":false}}])).unwrap(),
  permissions:vec!["data.read".into()],
  ai:serde_json::from_value(json!({"description":{"de-DE":"Pflege","it-IT":"Cura"},"tools":["list_guides"],"entities":["guides"],"ontology":[{"entity":"guides","nodeType":"care_advice","label":{"de-DE":"Pflege","it-IT":"Cura"},"fields":["product_id","care"]}]})).unwrap()
 }
    }
    pub fn context(&self) -> Context<'_> {
        Context {
            id: "care_graph",
            version: "1.0.0",
            entities: &self.entities,
            actions: &self.actions,
            permissions: &self.permissions,
            intelligence: Some(&self.ai),
        }
    }
}
