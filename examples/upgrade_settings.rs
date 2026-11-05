//! Run a standalone settings update and explicit model upgrade without Core, SQL, network or credentials.
use serde_json::json;
use veyndo_app_model::{
    Entity,
    evolution::{self, Migration},
    settings::{self, Setting},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings: Vec<Setting> = serde_json::from_value(json!([
        {"key":"label","label":{"en-GB":"Label","de-DE":"Beschriftung"},"kind":"string","required":true},
        {"key":"enabled","label":{"en-GB":"Enabled"},"kind":"boolean"}
    ]))?;
    let encoded = settings::prepare(
        &settings,
        &json!({"label":"Advice"}),
        &json!({"enabled":true}),
    )?;
    let values = serde_json::from_str(&encoded)?;
    println!(
        "Browser settings: {}",
        settings::visible(&settings, &values)
    );
    let old: Entity = serde_json::from_value(
        json!({"name":"advice","fields":[{"name":"title","kind":"string"}]}),
    )?;
    let next: Entity = serde_json::from_value(
        json!({"name":"advice","fields":[{"name":"heading","kind":"string"}]}),
    )?;
    let migration: Migration = serde_json::from_value(
        json!({"fromVersion":"1.0.0","steps":[{"op":"rename","entity":"advice","field":"title","to":"heading"}]}),
    )?;
    evolution::validate("2.0.0", std::slice::from_ref(&migration))?;
    let shape = evolution::shape(&[old], &[next], Some(&migration))?;
    println!("Reviewed model: {}", serde_json::to_string(&shape)?);
    Ok(())
}
