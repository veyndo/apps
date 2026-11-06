//! Explicit approved, package-bound compare-and-swap write and retained history wire contracts.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Write {
    pub app: String,
    pub action: String,
    pub package_digest: String,
    pub approve: bool,
    pub input: Input,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub id: String,
    pub revision: i64,
    pub fields: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub app: String,
    pub action: String,
    pub id: String,
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["app","action","packageDigest","approve","input"],"properties":{"app":{"type":"string","minLength":1,"maxLength":64},"action":{"type":"string","minLength":1,"maxLength":64},"packageDigest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"approve":{"type":"boolean","const":true},"input":{"type":"object","additionalProperties":false,"required":["id","revision","fields"],"properties":{"id":{"type":"string","minLength":1,"maxLength":100},"revision":{"type":"integer","minimum":0},"fields":{"type":"object"}}}}})
}
pub fn history_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["app","action","id"],"properties":{"app":{"type":"string","minLength":1,"maxLength":64},"action":{"type":"string","minLength":1,"maxLength":64},"id":{"type":"string","minLength":1,"maxLength":100}}})
}
pub fn tool(history: bool) -> Value {
    json!({"name":if history{"apps.records.history"}else{"apps.records.write"},"description":if history{"Read retained App record versions under current and historical rights"}else{"Explicitly approve one package-bound App record write with current rights and revision"},"inputSchema":if history{history_schema()}else{schema()},"annotations":{"readOnlyHint":history,"destructiveHint":!history,"idempotentHint":false,"openWorldHint":false}})
}
