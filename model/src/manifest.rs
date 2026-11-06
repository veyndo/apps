//! Complete strict native package wire model; decoding and fingerprints do not grant installation or service authority.
use crate::{
    Action, Entity, Slot,
    configuration::Contract as ConfigurationContract,
    intelligence::Contract as IntelligenceContract,
    surfaces::{ApiRoute, Surface},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    pub core_api: String,
    pub runtime: String,
    pub name: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribution: Option<crate::distribution::Distribution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<crate::presentation::Presentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<crate::business_capability::Contract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycles: Vec<crate::native_lifecycle::Contract>,
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<ConfigurationContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<crate::settings::Setting>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commerce_hooks: Option<crate::commerce_hooks::Contract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payment_provider: Option<crate::payment_provider::ProviderContract>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_filters: Vec<crate::events::EventFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_delivery: Option<crate::events::EventDelivery>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<Surface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub api_routes: Vec<ApiRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intelligence: Option<IntelligenceContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<crate::views::NativeView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<crate::triggers::AppSchedule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema_migrations: Vec<crate::evolution::Migration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub webhooks: Vec<crate::triggers::AppWebhook>,
}

impl Manifest {
    /// Borrow the common grammar from this complete typed package; domain and trust checks remain required compiler ports.
    pub fn common_contract(&self) -> crate::package::Contract<'_> {
        crate::package::Contract {
            id: &self.id,
            version: &self.version,
            core_api: &self.core_api,
            runtime: &self.runtime,
            category: self.category.as_deref(),
            permissions: &self.permissions,
            entities: &self.entities,
            actions: &self.actions,
            slots: &self.slots,
            events: &self.events,
            configuration: self
                .configuration
                .as_ref()
                .map(|c| crate::package::Configuration {
                    entity: &c.entity,
                    price_field: &c.price_field,
                    input_field: &c.input_field,
                    record: &c.record,
                    wasm_source: &c.wasm_source,
                }),
            has_payment_provider: self.payment_provider.is_some(),
        }
    }
}
/// Original sorted serde_json Value representation, independent of downstream preserve_order feature unification.
pub fn value_digest(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    fn ordered(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => {
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort();
                let mut output = serde_json::Map::new();
                for key in keys {
                    output.insert(key.clone(), ordered(&object[key]));
                }
                serde_json::Value::Object(output)
            }
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.iter().map(ordered).collect())
            }
            _ => value.clone(),
        }
    }
    format!(
        "{:x}",
        Sha256::digest(ordered(value).to_string().as_bytes())
    )
}
pub fn canonical_digest(manifest: &Manifest) -> String {
    value_digest(&serde_json::json!(manifest))
}
/// Clear only the signature; complete content, permissions, channel and dependencies remain covered.
pub fn signature_message(manifest: &Manifest) -> Option<Vec<u8>> {
    let mut copy = manifest.clone();
    copy.distribution.as_mut()?.signature.clear();
    Some(format!("veyndo-package-v1\n{}", canonical_digest(&copy)).into_bytes())
}
