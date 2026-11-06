//! Versioned typed simulation contract and one HTTP/MCP operation description; results never confer live commerce authority.
use crate::error::{Problem, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use veyndo_app_model::{business_capability::Expected, business_snapshot::Frozen};
#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Hook {
    Price,
    Shipping,
    Discount,
    Validation,
}
impl Hook {
    pub fn runtime(self) -> veyndo_app_runtime::component_runtime::Kind {
        use veyndo_app_runtime::component_runtime::Kind;
        match self {
            Self::Price => Kind::Price,
            Self::Shipping => Kind::Shipping,
            Self::Discount => Kind::Discount,
            Self::Validation => Kind::Validation,
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Evaluation {
    pub source: String,
    pub hook: Hook,
    pub snapshot: Frozen,
    pub context: Value,
}
impl Evaluation {
    pub fn validate(&self) -> Result<()> {
        let bad = || {
            Problem::new(
                axum::http::StatusCode::BAD_REQUEST,
                "invalid_simulation",
                "Check the source, JSON context, snapshot identifiers, amounts and size limits in /v1/contract",
            )
        };
        if self.source.is_empty()
            || self.source.len() > 32768
            || !self.context.is_object()
            || self.context.to_string().len() > 8192
        {
            return Err(bad());
        }
        self.snapshot.validate_bounds().map_err(|_| bad())?;
        if self.snapshot.records.iter().any(|r| {
            !veyndo_app_model::declarations::identifier(&r.entity)
                || r.id.is_empty()
                || r.id.len() > 100
                || r.revision == 0
                || !r.fields.is_object()
        }) || self
            .snapshot
            .records
            .iter()
            .map(|r| r.fields.to_string().len())
            .sum::<usize>()
            > 65536
        {
            return Err(bad());
        }
        Ok(())
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationResult {
    pub contract_version: u8,
    pub mode: &'static str,
    pub effect_applied: bool,
    pub source_digest: String,
    pub input_digest: String,
    pub hook: Hook,
    pub outcome: Expected,
}
pub fn input_schema() -> Value {
    let amount = json!({"type":"object","additionalProperties":false,"required":["minor","currency","scale"],"properties":{"minor":{"type":"integer","minimum":0,"maximum":9007199254740991_i64},"currency":{"type":"string","pattern":"^[A-Z]{3}$"},"scale":{"type":"integer","minimum":0,"maximum":3}}});
    let id = json!({"type":"string","minLength":1,"maxLength":100});
    json!({"type":"object","additionalProperties":false,"required":["source","hook","snapshot","context"],"properties":{
        "source":{"type":"string","minLength":1,"maxLength":32768},"hook":{"enum":["price","shipping","discount","validation"]},"context":{"type":"object"},
        "snapshot":{"type":"object","additionalProperties":false,"required":["subtotal"],"properties":{
            "subtotal":amount,"items":{"type":"array","maxItems":100,"items":{"type":"object","additionalProperties":false,"required":["id","quantity","price"],"properties":{"id":id,"quantity":{"type":"integer","minimum":1,"maximum":4294967295_u64},"price":amount}}},
            "products":{"type":"array","maxItems":100,"items":{"type":"object","additionalProperties":false,"required":["id","stock","price"],"properties":{"id":id,"stock":{"type":"integer","minimum":0,"maximum":4294967295_u64},"price":amount}}},
            "records":{"type":"array","maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["entity","id","revision","fields"],"properties":{"entity":{"type":"string","pattern":"^[a-z][a-z0-9_]{0,31}$"},"id":id,"revision":{"type":"integer","minimum":1},"fields":{"type":"object"}}}}
        }}
    }})
}
pub fn output_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["contractVersion","mode","effectApplied","sourceDigest","inputDigest","hook","outcome"],"properties":{
        "contractVersion":{"const":1},"mode":{"const":"simulation"},"effectApplied":{"const":false},"sourceDigest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"inputDigest":{"type":"string","pattern":"^[a-f0-9]{64}$"},"hook":{"enum":["price","shipping","discount","validation"]},
        "outcome":{"type":"object","additionalProperties":false,"required":["adjustment","accepted","reasonCode"],"properties":{"accepted":{"type":"boolean"},"reasonCode":{"type":"string","pattern":"^[a-zA-Z0-9_]{0,100}$"},"adjustment":{"type":"object","additionalProperties":false,"required":["minor","currency","scale"],"properties":{"minor":{"type":"integer","minimum":-100000000,"maximum":100000000},"currency":{"type":"string","pattern":"^[A-Z]{3}$"},"scale":{"type":"integer","minimum":0,"maximum":3}}}}}
    }})
}
pub fn tool() -> Value {
    json!({"name":"apps.evaluate","description":"Run a read-only Wasm commerce simulation using supplied example data. No live records or effects.","inputSchema":input_schema(),"outputSchema":output_schema(),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
}
pub fn document() -> Value {
    json!({"version":1,"service":"veyndo-app-runtime","scope":"apps.evaluate","http":{"method":"POST","path":"/v1/evaluations"},"mcp":{"path":"/mcp","tool":tool()},"inputSchema":input_schema(),"outputSchema":output_schema(),"limits":{"sourceBytes":32768,"contextBytes":8192,"recordBytes":65536,"bodyBytes":131072,"globalExecutions":4,"tenantExecutions":2,"subjectExecutions":1},"effects":[]})
}
/// OpenAPI and MCP are projections of the same operation schemas, never separate validation/dispatch paths.
pub fn openapi() -> Value {
    let mut responses = serde_json::Map::new();
    responses.insert("200".into(),json!({"description":"Read-only simulation result","content":{"application/json":{"schema":output_schema()}}}));
    let problem = json!({"type":"object","required":["status","code","detail"],"properties":{"status":{"type":"integer"},"code":{"type":"string"},"detail":{"type":"string"}}});
    for status in [400, 401, 403, 413, 415, 422, 429, 503, 504] {
        responses.insert(status.to_string(),json!({"description":"Sanitized problem; no commerce effect","content":{"application/json":{"schema":problem}}}));
    }
    json!({"openapi":"3.1.0","info":{"title":"Veyndo App Runtime","version":env!("CARGO_PKG_VERSION")},"servers":[{"url":"/"}],"components":{"securitySchemes":{"bearer":{"type":"http","scheme":"bearer"}}},"security":[{"bearer":[]}],"paths":{"/v1/evaluations":{"post":{"operationId":"apps.evaluate","summary":"Evaluate a synthetic read-only commerce hook","requestBody":{"required":true,"content":{"application/json":{"schema":input_schema()}}},"responses":responses}}}})
}
