//! Strict localized native view and control wire types shared by visual, agent and API editors.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeView {
    pub id: String,
    pub layout: String,
    pub blocks: Vec<NativeBlock>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeBlock {
    pub id: String,
    pub kind: String,
    pub title: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub text: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_binding: Option<ContextBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<crate::form_layout::Geometry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_order: Option<u16>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tooltip: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_blocks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handlers: Option<crate::ui_ast::Handlers>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inline_edit: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextBinding {
    pub field: String,
    pub key: String,
}
