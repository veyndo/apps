//! Compile a native editor and its behavior independently; execution and current identity belong to the embedding host.
use serde_json::{Value, json};
use veyndo_app_model::{
    fields::ValuePolicy,
    views::{self, Contract, Policy},
};
struct Host;
impl ValuePolicy for Host {
    fn locale(&self, key: &str) -> bool {
        ["en-GB", "de-DE"].contains(&key)
    }
    // The example has no money, rich-content or relation fields; unsupported host features are denied.
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
impl Policy for Host {
    fn public_read(&self, writes: bool) -> bool {
        !writes
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let entities = serde_json::from_value::<Vec<_>>(
        json!([{"name":"notes","publicRead":false,"fields":[{"name":"title","kind":"string","required":true}]}]),
    )?;
    let actions = serde_json::from_value::<Vec<_>>(json!([
        {"name":"read","description":"Read notes","handler":"list","entity":"notes","public":false,"inputSchema":{"type":"object"}},
        {"name":"save","description":"Save note","handler":"save","entity":"notes","public":false,"inputSchema":{"type":"object"}}
    ]))?;
    let surfaces = serde_json::from_value::<Vec<_>>(
        json!([{"id":"manager","location":"admin.navigation","label":{"en-GB":"Notes","de-DE":"Notizen"},"uiPath":"native/manager","actions":["read","save"]}]),
    )?;
    let views = serde_json::from_value::<Vec<_>>(
        json!([{"id":"manager","layout":"form","blocks":[
            {"id":"editor","kind":"form","title":{"en-GB":"Edit note","de-DE":"Notiz bearbeiten"},"entity":"notes","readAction":"read","writeAction":"save","geometry":{"x":0,"y":0,"w":12,"h":6}},
            {"id":"submit","kind":"button","title":{"en-GB":"Save","de-DE":"Speichern"},"handlers":{"click":[{"op":"validate","target":"editor"},{"op":"call","action":"save","input":{"kind":"record","block":"editor"}}]}}
        ]}]),
    )?;
    let contract = Contract {
        entities: &entities,
        actions: &actions,
        surfaces: &surfaces,
        views: &views,
    };
    views::validate(&contract, &Host)?;
    let payload = views::payload(&contract, &surfaces[0]).ok_or("Missing view")?;
    assert_eq!(
        payload["view"]["blocks"][1]["handlers"]["click"][1]["action"],
        "save"
    );
    println!("Compiled localized native form and typed save behavior with the shared model");
    Ok(())
}
