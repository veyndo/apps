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
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration: Option<ConfigurationContract>,
    #[serde(default)]
    pub entities: Vec<Entity>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub events: Vec<String>,
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
}
pub(crate) fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 32
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        && s.as_bytes()[0].is_ascii_lowercase()
}
pub(crate) fn validate(m: &Manifest) -> Result<()> {
    if !identifier(&m.id)
        || m.core_api != "1"
        || !["declarative", "service"].contains(&m.runtime.as_str())
        || m.version.split('.').count() != 3
        || !m.version.split('.').all(|v| v.parse::<u32>().is_ok())
        || m.entities.len() > 12
        || m.actions.len() > 24
        || m.slots.len() > 12
        || m.events.len() > 12
    {
        return Err(bad(
            "Unsupported app identity, API version, runtime or package limits",
        ));
    }
    if m.permissions.iter().any(|p| {
        ![
            "data.read",
            "data.write",
            "storefront.slot",
            "admin.slot",
            "service.call",
            "events.read",
        ]
        .contains(&p.as_str())
    }) {
        return Err(bad("Unknown app permission"));
    }
    let mut names = std::collections::HashSet::new();
    for e in &m.entities {
        if !identifier(&e.name)
            || !names.insert(&e.name)
            || e.fields.is_empty()
            || e.fields.len() > 16
        {
            return Err(bad("Invalid entity"));
        }
        let mut fields = std::collections::HashSet::new();
        for f in &e.fields {
            if !identifier(&f.name)
                || ["tenant", "id", "revision"].contains(&f.name.as_str())
                || !fields.insert(&f.name)
                || !["string", "integer", "boolean"].contains(&f.kind.as_str())
                || (f.translatable && (f.kind != "string" || f.references.is_some()))
                || f.references
                    .as_ref()
                    .is_some_and(|r| f.kind != "string" || !m.entities.iter().any(|e| e.name == *r))
            {
                return Err(bad("Invalid field or entity relationship"));
            }
        }
    }
    if let Some(c) = &m.configuration
        && (!identifier(&c.entity)
            || !identifier(&c.price_field)
            || !identifier(&c.input_field)
            || c.record.is_empty()
            || c.record.len() > 100
            || c.wasm_source.len() > 32768
            || !m.permissions.contains(&"data.read".into())
            || !m.permissions.contains(&"data.write".into())
            || !m.entities.iter().any(|e| {
                e.name == c.entity
                    && e.fields
                        .iter()
                        .any(|f| f.name == c.price_field && f.kind == "integer" && f.required)
            }))
    {
        return Err(bad("Invalid app configuration contract"));
    }
    let mut actions = std::collections::HashSet::new();
    for a in &m.actions {
        if !identifier(&a.name)
            || !actions.insert(&a.name)
            || a.description.len() > 300
            || !["list", "save", "service", "configurations"].contains(&a.handler.as_str())
            || a.input_schema["type"] != "object"
            || a.input_schema["additionalProperties"] != false
            || (a.handler == "save" && a.public)
            || (a.handler == "configurations" && a.public)
            || a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n))
        {
            return Err(bad("Invalid app action"));
        }
        if ["list", "save"].contains(&a.handler.as_str()) && a.entity.is_none() {
            return Err(bad("Entity action requires entity"));
        }
        if a.public
            && a.entity
                .as_ref()
                .is_some_and(|n| !m.entities.iter().any(|e| e.name == *n && e.public_read))
        {
            return Err(bad("Private entity cannot be a public tool"));
        }
    }
    for s in &m.slots {
        if !["product.detail", "admin.apps", "admin.order"].contains(&s.location.as_str())
            || ![
                "entity-form",
                "entity-list",
                "engraving",
                "product-configuration",
                "payments",
                "iframe",
            ]
            .contains(&s.component.as_str())
        {
            return Err(bad(
                "Unsupported UI slot; executable JavaScript is not accepted",
            ));
        }
    }
    if !m.events.is_empty()
        && (!m.permissions.contains(&"events.read".into()) || m.runtime != "service")
    {
        return Err(bad(
            "Event subscriptions require service runtime and events.read",
        ));
    }
    Ok(())
}
pub(crate) fn table(app: &str, entity: &str) -> String {
    format!("app_{}_{}", &hash(app)[..16], entity)
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
    fn built_in_packages_are_valid() {
        for s in [
            include_str!("../../extensions/apps/engraving/manifest.json"),
            include_str!("../../extensions/apps/paypal/manifest.json"),
            include_str!("../../extensions/apps/gift-message/manifest.json"),
            include_str!("../../extensions/apps/storyfront/manifest.json"),
        ] {
            validate(&serde_json::from_str(s).unwrap()).unwrap();
        }
    }
}
