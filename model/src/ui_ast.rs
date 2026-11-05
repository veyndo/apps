//! Strict bounded UI behavior syntax; serialized declarations confer no action authority.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Expression {
    Literal {
        value: Value,
    },
    Value {
        block: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    Record {
        block: String,
    },
    Object {
        fields: HashMap<String, Expression>,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Statement {
    Set {
        target: String,
        value: Expression,
    },
    If {
        left: Expression,
        compare: String,
        right: Expression,
        then: Vec<Statement>,
        #[serde(default)]
        otherwise: Vec<Statement>,
    },
    Call {
        action: String,
        input: Expression,
    },
    MsgBox {
        text: HashMap<String, String>,
    },
    Navigate {
        view: String,
    },
    Refresh {
        target: String,
    },
    Validate {
        target: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Handlers {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub click: Vec<Statement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub change: Vec<Statement>,
}
