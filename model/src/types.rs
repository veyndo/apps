//! Exact native entity/field/extension wire types; serialization defaults preserve immutable package digests.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entity {
    /// Runtime receipts and locks never travel with sandbox configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operational: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<Extension>,
    pub name: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub label: HashMap<String, String>,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub public_read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<crate::publication::Publication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<crate::lifecycle::Workflow>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Field {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub translatable: bool,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub label: HashMap<String, String>,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub indexed: bool,
    #[serde(default)]
    pub references: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unique: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<Value>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Choice {
    pub value: String,
    pub label: HashMap<String, String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Extension {
    pub version: u8,
    pub object: Object,
    pub field: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Object {
    Product,
    Customer,
    Order,
    Company,
}
impl Object {
    pub fn reference(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Customer => "customer",
            Self::Order => "order",
            Self::Company => "b2bCompany",
        }
    }
    pub fn scopes(self, write: bool) -> &'static [&'static str] {
        match (self, write) {
            (Self::Product, false) => &["catalog.read"],
            (Self::Product, true) => &["catalog.read", "catalog.write"],
            (Self::Customer, false) => &["customers.read", "customers.pii"],
            (Self::Customer, true) => &["customers.read", "customers.pii", "customers.write"],
            (Self::Order, false) => &["orders.read", "customers.pii"],
            (Self::Order, true) => &["orders.read", "customers.pii", "orders.write"],
            (Self::Company, false) => &["b2b.read"],
            (Self::Company, true) => &["b2b.read", "b2b.manage"],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Action {
    pub name: String,
    pub description: String,
    pub handler: String,
    pub entity: Option<String>,
    pub input_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    #[serde(default)]
    pub public: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flow_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<bool>,
}
