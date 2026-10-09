//! Strict package contract; identifiers and limits are checked before any schema DDL.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Manifest {
    pub id: String,
    pub version: String,
    pub core_api: String,
    pub runtime: String,
    pub name: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<super::distribution::Distribution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<super::presentation::Presentation>,
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<ConfigurationContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commerce_hooks: Option<super::commerce_hooks::Contract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_provider: Option<crate::payments::ProviderContract>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_filters: Vec<event_contract::EventFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_delivery: Option<event_contract::EventDelivery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<Surface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_routes: Vec<ApiRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intelligence: Option<IntelligenceContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<super::native_views::NativeView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<super::schedules::AppSchedule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema_migrations: Vec<super::schema_changes::Migration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhooks: Vec<super::webhooks::AppWebhook>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Surface {
    pub id: String,
    pub location: String,
    pub label: HashMap<String, String>,
    pub ui_path: String,
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ApiRoute {
    pub path: String,
    pub method: String,
    pub scope: String,
    pub action: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct IntelligenceContract {
    pub description: HashMap<String, String>,
    pub tools: Vec<String>,
    pub entities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ontology: Vec<super::ontology::Mapping>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfigurationContract {
    pub entity: String,
    pub record: String,
    pub price_field: String,
    pub input_field: String,
    pub wasm_source: String,
    pub default_fields: Value,
    pub label: HashMap<String, String>,
    pub hint: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entity {
    pub name: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub label: HashMap<String, String>,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub public_read: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Field {
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
pub(crate) struct Choice {
    pub value: String,
    pub label: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Slot {
    pub location: String,
    pub component: String,
    pub label: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Action {
    pub name: String,
    pub description: String,
    pub handler: String,
    pub entity: Option<String>,
    pub input_schema: Value,
    #[serde(default)]
    pub public: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_only: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flow_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<bool>,
}
pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        && s.as_bytes()[0].is_ascii_lowercase()
}
/// Quote a contract-validated field so SQL keywords remain usable as app field names.
pub(crate) fn column(name: &str) -> String {
    debug_assert!(identifier(name));
    format!("\"{name}\"")
}
pub(crate) fn table(tenant: &str, app: &str, entity: &str) -> String {
    format!("app_{}_{}", &hash(&format!("{tenant}:{app}"))[..24], entity)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identifiers_cannot_inject_sql() {
        assert!(identifier("engraving"));
        for s in ["x;drop", "tenant--", "1abc", "x.y", "é"] {
            assert!(!identifier(s));
        }
    }
    #[test]
    fn order_configurations_are_never_public() {
        let mut m: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        ))
        .unwrap();
        m.actions
            .iter_mut()
            .find(|a| a.handler == "configurations")
            .unwrap()
            .public = true;
        assert!(validate(&m).is_err());
    }
    #[test]
    fn executable_package_matches_app_source() {
        let m: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        ))
        .unwrap();
        assert_eq!(
            m.configuration.unwrap().wasm_source,
            include_str!("../../extensions/apps/engraving/configuration.wat")
        );
    }
    #[test]
    fn additive_action_metadata_does_not_change_published_legacy_digests() {
        let m: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/engraving/manifest.json"
        ))
        .unwrap();
        let value = json!(m);
        assert!(
            value["actions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|a| a.get("readOnly").is_none() && a.get("permission").is_none())
        );
    }
    #[test]
    fn built_in_packages_are_valid() {
        for s in [
            include_str!("../../extensions/apps/engraving/manifest.json"),
            include_str!("../../extensions/apps/paypal/manifest.json"),
            include_str!("../../extensions/apps/gift-message/manifest.json"),
            include_str!("../../extensions/apps/storyfront/manifest.json"),
            include_str!("../../extensions/apps/google-analytics/manifest.json"),
            include_str!("../../extensions/apps/gmail/manifest.json"),
            include_str!("../../extensions/apps/slack/manifest.json"),
            include_str!("../../extensions/apps/email/manifest.json"),
        ] {
            validate(&serde_json::from_str(s).unwrap()).unwrap();
        }
    }
}
