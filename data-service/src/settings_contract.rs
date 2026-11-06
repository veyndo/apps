//! Strict administrator settings contracts reused by HTTP, OpenAPI and MCP; secret values have no read projection.
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub app: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Save {
    pub app: String,
    pub digest: String,
    pub revision: i64,
    pub approve: bool,
    pub values: Value,
}
pub fn schema(save: bool) -> Value {
    let mut value = json!({"type":"object","additionalProperties":false,"required":["app"],"properties":{"app":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"}}});
    if save {
        value["required"] = json!(["app", "digest", "revision", "approve", "values"]);
        for (key, field) in [
            (
                "digest",
                json!({"type":"string","pattern":"^[a-f0-9]{64}$"}),
            ),
            ("revision", json!({"type":"integer","minimum":0})),
            ("approve", json!({"type":"boolean","const":true})),
            ("values", json!({"type":"object","maxProperties":16})),
        ] {
            value["properties"][key] = field;
        }
    }
    value
}
pub fn tool(save: bool) -> Value {
    json!({"name":if save {"apps.settings.save"}else{"apps.settings.read"},"description":if save {"Explicitly approve a current package/revision-bound settings update as its personal live administrator; secrets stay encrypted"}else{"Read declared App settings and configured key names as its personal live administrator; secret values are never returned"},"inputSchema":schema(save),"annotations":{"readOnlyHint":!save,"destructiveHint":save,"idempotentHint":false,"openWorldHint":false}})
}
