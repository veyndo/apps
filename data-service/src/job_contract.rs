//! Strict job-supervision wire contracts shared by HTTP, OpenAPI, MCP and the generic SDK.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub app: String,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<i64>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub after: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub group: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub package_digest: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub revision: Option<i64>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub operation: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub approve: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::contract::present",
        skip_serializing_if = "Option::is_none"
    )]
    pub approve_unknown_outcome: Option<bool>,
}
pub fn schema(op: &str) -> Value {
    let mut properties =
        json!({"app":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"}});
    let required = if op == "list" {
        properties["limit"] = json!({"type":"integer","minimum":1,"maximum":100});
        for k in ["after", "group"] {
            properties[k] = json!({"type":"string","minLength":1,"maxLength":100});
        }
        json!(["app"])
    } else {
        properties["id"] = json!({"type":"string","minLength":1,"maxLength":100});
        if op == "control" {
            properties["packageDigest"] = json!({"type":"string","pattern":"^[a-f0-9]{64}$"});
            properties["revision"] = json!({"type":"integer","minimum":1});
            properties["operation"] =
                json!({"type":"string","enum":["cancel","retry","resolve_cancelled","archive"]});
            properties["approve"] = json!({"type":"boolean","const":true});
            properties["approveUnknownOutcome"] = json!({"type":"boolean"});
            json!([
                "app",
                "id",
                "packageDigest",
                "revision",
                "operation",
                "approve"
            ])
        } else {
            json!(["app", "id"])
        }
    };
    json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
}
pub fn tool(op: &str) -> Value {
    json!({"name":format!("apps.jobs.{op}"),"description":match op{"list"=>"Read bounded job history with current-package/current-rights result redaction","detail"=>"Read an owned or administrator-visible job under its current action rights",_=>"Explicitly cancel, retry, resolve or archive an existing App job using current personal authority and revision approval"},"inputSchema":schema(op),"annotations":{"readOnlyHint":op!="control","destructiveHint":op=="control","idempotentHint":op!="control","openWorldHint":op=="control"}})
}
