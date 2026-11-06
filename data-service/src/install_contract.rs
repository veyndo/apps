//! Exact reviewed package/permission/version CAS wire used by HTTP, OpenAPI and MCP.
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub manifest: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    pub manifest: Value,
    pub digest: String,
    pub revision: i64,
    pub approve: bool,
    pub permissions: Vec<String>,
}
pub fn schema(install: bool) -> Value {
    let mut doc = json!({"type":"object","additionalProperties":false,"required":["manifest"],"properties":{"manifest":{"type":"object"}}});
    if install {
        doc["required"] = json!(["manifest", "digest", "revision", "approve", "permissions"]);
        for (key, value) in [
            (
                "digest",
                json!({"type":"string","pattern":"^[a-f0-9]{64}$"}),
            ),
            ("revision", json!({"type":"integer","minimum":0})),
            ("approve", json!({"type":"boolean","const":true})),
            (
                "permissions",
                json!({"type":"array","maxItems":2,"uniqueItems":true,"items":{"type":"string"}}),
            ),
        ] {
            doc["properties"][key] = value;
        }
    }
    doc
}
pub fn tool(install: bool) -> Value {
    json!({"name":if install{"apps.packages.install"}else{"apps.packages.review"},"description":if install{"Explicitly approve and atomically install one fully compiled portable package at the current revision as a personal live administrator"}else{"Compile and review a complete private portable package, current installed revision and exact requested permissions"},"inputSchema":schema(install),"annotations":{"readOnlyHint":!install,"destructiveHint":install,"idempotentHint":false,"openWorldHint":false}})
}
