//! One strict operation contract shared by HTTP, MCP, Fetch and the generic Veyndo SDK.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub app: String,
    pub action: String,
    pub query: Query,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<i64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub after: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
}
fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}
impl Query {
    pub fn value(&self) -> Value {
        serde_json::to_value(self).expect("typed query serialization")
    }
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["app","action","query"],"properties":{"app":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"},"action":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"},"query":{"type":"object","additionalProperties":false,"properties":{"limit":{"type":"integer","minimum":1,"maximum":100,"default":100},"after":{"type":"string","maxLength":100},"id":{"type":"string","minLength":1,"maxLength":100}}}}})
}
pub fn tool() -> Value {
    json!({"name":"apps.records.read","description":"Read declared fields of one currently installed private App list action using current merchant permissions","inputSchema":schema(),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
}
pub fn document() -> Value {
    json!({"version":1,"service":"veyndo-app-data","operation":"apps.records.read","schema":schema(),"authority":"fresh current merchant OAuth context; signed read-only PostgreSQL scope","writes":false})
}
pub fn openapi() -> Value {
    json!({"openapi":"3.1.0","info":{"title":"Veyndo App Data","version":env!("CARGO_PKG_VERSION")},"paths":{"/v1/records/query":{"post":{"operationId":"apps.records.read","security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":schema()}}},"responses":{"200":{"description":"Bounded declared-field record page"},"400":{"description":"Invalid query or installed action schema"},"401":{"description":"Inactive or invalid token"},"403":{"description":"Current permission or MCP grant denied"},"404":{"description":"Installed app or action unavailable"},"422":{"description":"A native object resolver is required"},"503":{"description":"Identity or data service unavailable"}}}}},"components":{"securitySchemes":{"bearer":{"type":"http","scheme":"bearer"}}}})
}
