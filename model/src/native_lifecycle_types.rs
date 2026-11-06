//! Exact native lifecycle declarations, graph policies and accepted snapshot wire types; data alone confers no authority.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contract {
    pub version: u8,
    pub object: String,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub id: String,
    pub label: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transition {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub app: String,
    pub version: String,
    pub digest: String,
    pub required_permissions: Vec<String>,
    pub guards: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub version: u8,
    pub object: String,
    pub revision: i64,
    pub main_locale: String,
    pub machine: Machine,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Machine {
    pub states: Vec<MachineState>,
    pub transitions: Vec<MachineEdge>,
    #[serde(default)]
    pub source_app: Option<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachineState {
    pub id: String,
    pub label: HashMap<String, String>,
    pub terminal: bool,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachineEdge {
    pub id: String,
    pub from: String,
    pub to: String,
    pub label: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<Policy>,
}
