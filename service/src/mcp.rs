//! Stateless MCP JSON-RPC adapter over the same authenticated typed evaluation; tool descriptions never grant execution authority.
use crate::{
    App, contract,
    error::{Problem, Result},
    execution,
    identity::Principal,
};
use axum::{
    Extension, Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
fn rpc_error(id: Value, code: i64, message: &str) -> Response {
    Json(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})).into_response()
}
pub async fn handle(
    State(app): State<App>,
    Extension(p): Extension<Principal>,
    headers: HeaderMap,
    body: std::result::Result<Json<Value>, axum::extract::rejection::JsonRejection>,
) -> Result<Response> {
    if headers
        .get("mcp-protocol-version")
        .is_some_and(|v| !v.to_str().ok().is_some_and(|v| VERSIONS.contains(&v)))
    {
        return Err(Problem::new(
            StatusCode::BAD_REQUEST,
            "protocol_version",
            "Use MCP 2025-11-25, 2025-06-18 or 2025-03-26",
        ));
    }
    let v = body
        .map_err(|_| {
            Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_json",
                "Provide a JSON-RPC object",
            )
        })?
        .0;
    let id = v.get("id").cloned().unwrap_or(Value::Null);
    let fail = |code, message| Ok(rpc_error(id.clone(), code, message));
    if !v.is_object()
        || v["jsonrpc"] != "2.0"
        || v["method"].as_str().is_none()
        || v.as_object().is_some_and(|o| {
            o.keys()
                .any(|k| !["jsonrpc", "id", "method", "params"].contains(&k.as_str()))
        })
    {
        return fail(-32600, "Invalid JSON-RPC request");
    }
    if v["method"] == "notifications/initialized" && v.get("id").is_none() {
        return Ok(StatusCode::ACCEPTED.into_response());
    }
    if !(id.as_str().is_some_and(|s| s.len() <= 100) || id.is_i64() || id.is_u64()) {
        return Ok((
            StatusCode::BAD_REQUEST,
            rpc_error(
                id.clone(),
                -32600,
                "Request ID must be a bounded string or integer",
            ),
        )
            .into_response());
    }
    let result = match v["method"].as_str().unwrap() {
        "initialize" => {
            let version = v["params"]["protocolVersion"]
                .as_str()
                .unwrap_or(VERSIONS[0]);
            let version = if VERSIONS.contains(&version) {
                version
            } else {
                VERSIONS[0]
            };
            json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"veyndo-app-runtime","version":env!("CARGO_PKG_VERSION")}})
        }
        "ping" => json!({}),
        "tools/list" => {
            if v.get("params")
                .is_some_and(|p| !p.is_object() || p.as_object().is_some_and(|o| !o.is_empty()))
            {
                return fail(-32602, "No tool cursor or other parameters are supported");
            }
            json!({"tools":[contract::tool()]})
        }
        "tools/call" => {
            if v["params"]["name"] != "apps.evaluate"
                || v["params"].as_object().is_none_or(|o| {
                    o.keys()
                        .any(|k| !["name", "arguments"].contains(&k.as_str()))
                })
            {
                return fail(-32602, "Unknown tool or parameters");
            }
            let Ok(input) =
                serde_json::from_value::<contract::Evaluation>(v["params"]["arguments"].clone())
            else {
                return fail(-32602, "Arguments must match the evaluation contract");
            };
            match execution::evaluate(&app, p, input).await {
                Ok(value) => {
                    let value = json!(value);
                    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                }
                Err(problem) => {
                    json!({"content":[{"type":"text","text":problem.value().to_string()}],"isError":true})
                }
            }
        }
        _ => return fail(-32601, "Method not found"),
    };
    Ok(Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response())
}
