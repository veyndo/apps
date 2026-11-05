//! Validate an app-owned product extension and derive a graph from synthetic already-authorized rows without Core or SQL.
use serde_json::json;
use veyndo_app_model::{
    Action, Entity,
    intelligence::{self, Context, Contract},
    object_extensions, ontology,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let entities: Vec<Entity> = serde_json::from_value(
        json!([{"name":"care","extends":{"version":1,"object":"product","field":"owner"},"label":{"de-DE":"Pflege","it-IT":"Cura"},"fields":[{"name":"owner","kind":"string","required":true,"indexed":true,"unique":true,"coreReference":"product"},{"name":"advice","kind":"string"},{"name":"internal","kind":"string"}]}]),
    )?;
    let actions: Vec<Action> = serde_json::from_value(
        json!([{"name":"read","description":"Read","handler":"list","entity":"care","requiredPermissions":["catalog.read"],"inputSchema":{"type":"object","additionalProperties":false}}]),
    )?;
    let permissions = vec!["data.read".into()];
    object_extensions::validate(&entities, &actions, &permissions)?;
    let ai: Contract = serde_json::from_value(
        json!({"description":{"en-GB":"Care advice"},"tools":["read"],"entities":["care"],"ontology":[{"entity":"care","nodeType":"care_advice","label":{"en-GB":"Care"},"fields":["owner","advice"],"relations":{"owner":"applies_to"}}]}),
    )?;
    intelligence::validate(Some(&ai), &actions, &entities)?;
    let context = Context {
        id: "care_example",
        version: "1.0.0",
        entities: &entities,
        actions: &actions,
        permissions: &permissions,
        intelligence: Some(&ai),
    };
    ontology::validate(&context)?;
    // A real host must obtain these rows through its current authorized tenant-scoped list.
    let rows = json!([{"id":"one","revision":3,"owner":"mug","advice":"Hand wash","internal":"not selected"}]);
    let graph = ontology::project("example_tenant", &context, &entities[0], &rows).unwrap();
    assert!(graph["nodes"][0]["properties"].get("internal").is_none());
    println!("{}", serde_json::to_string_pretty(&graph)?);
    Ok(())
}
