//! Structured cart configuration uses the canonical field validator and declared bounded integer guest arguments.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Inputs {
    pub fields: Vec<Field>,
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Example>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Example {
    pub id: String,
    pub label: HashMap<String, String>,
    pub fields: Value,
    pub material_fee_minor: i64,
    #[serde(deserialize_with = "expected_fee")]
    pub expected_fee_minor: Option<i64>,
}
fn expected_fee<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<i64>, D::Error> {
    Option::<i64>::deserialize(deserializer)
}
impl Inputs {
    fn entity(&self) -> Result<Entity> {
        serde_json::from_value(json!({"name":"configuration","fields":self.fields}))
            .map_err(|_| bad("Invalid configuration fields"))
    }
    pub fn arguments(&self, fields: &Value) -> Result<Vec<i64>> {
        data::fields(&self.entity()?, fields)?;
        for value in fields
            .as_object()
            .ok_or(bad("Configuration object required"))?
            .values()
        {
            if value
                .as_str()
                .is_some_and(|s| s.chars().any(char::is_control))
            {
                return Err(bad("Printable configuration fields required"));
            }
        }
        if self.arguments.iter().any(|name| {
            fields[name]
                .as_i64()
                .is_some_and(|value| value.unsigned_abs() > 9_007_199_254_740_991)
        }) {
            return Err(bad("Configuration integers exceed exact JSON wire range"));
        }
        self.arguments
            .iter()
            .map(|name| {
                fields[name]
                    .as_i64()
                    .or_else(|| fields[name].as_bool().map(i64::from))
                    .ok_or(bad("Declared integer configuration argument required"))
            })
            .collect()
    }
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    let Some(c) = &m.configuration else {
        return Ok(());
    };
    if c.currency
        .as_deref()
        .is_some_and(|code| currencies::scale(code).is_none())
    {
        return Err(bad("Unknown configuration currency"));
    }
    let Some(inputs) = &c.inputs else {
        return Ok(());
    };
    if inputs.fields.is_empty()
        || inputs.fields.len() > 8
        || inputs.arguments.is_empty()
        || inputs.arguments.len() > 8
    {
        return Err(bad(
            "Structured configuration needs one to eight fields and integer arguments",
        ));
    }
    let mut names = std::collections::HashSet::new();
    for f in &inputs.fields {
        if !identifier(&f.name)
            || !names.insert(&f.name)
            || !f.required
            || f.translatable
            || f.core_reference.is_some()
            || f.indexed
            || f.unique
            || !["integer", "boolean", "string"].contains(&f.kind.as_str())
            || f.references
                .as_ref()
                .is_some_and(|r| Some(&f.name) != inputs.record_field.as_ref() || r != &c.entity)
            || !business_capability::localized(&f.label)
        {
            return Err(bad("Invalid structured configuration field"));
        }
    }
    let mut cases = std::collections::HashSet::new();
    if inputs.examples.len() > 12
        || json!(inputs).to_string().len() > 32768
        || inputs.examples.iter().any(|case| {
            !identifier(&case.id)
                || !cases.insert(&case.id)
                || !business_capability::localized(&case.label)
                || !case.fields.is_object()
                || case.material_fee_minor < 0
                || case.material_fee_minor > 100_000_000
                || case
                    .expected_fee_minor
                    .is_some_and(|fee| !(0..=100_000_000).contains(&fee))
        })
    {
        return Err(bad(
            "Invalid frozen configuration example or example budget",
        ));
    }
    let entity = inputs.entity()?;
    editor_contract::validate_fields(&entity)?;
    field_values::contract(&entity)?;
    let mut arguments = std::collections::HashSet::new();
    if inputs.arguments.iter().any(|name| {
        !arguments.insert(name)
            || !inputs
                .fields
                .iter()
                .any(|f| f.name == *name && ["integer", "boolean"].contains(&f.kind.as_str()))
    }) || inputs.record_field.as_ref().is_some_and(|name| {
        !inputs.fields.iter().any(|f| {
            f.name == *name
                && f.kind == "string"
                && f.references.as_deref() == Some(c.entity.as_str())
        })
    }) {
        return Err(bad(
            "Configuration arguments or library reference do not match declared fields",
        ));
    }
    if inputs.record_field.is_some()
        && !m
            .entities
            .iter()
            .any(|e| e.name == c.entity && e.public_read)
    {
        return Err(bad(
            "Selectable configuration library must permit declared public reads",
        ));
    }
    if inputs.record_field.is_some()
        && !m.actions.iter().any(|action| {
            action.handler == "list"
                && action.public
                && action.entity.as_deref() == Some(c.entity.as_str())
                && ["id", "after"]
                    .iter()
                    .all(|key| action.input_schema["properties"][key]["type"] == "string")
                && action.input_schema["properties"]["limit"]["type"] == "integer"
        })
    {
        return Err(bad(
            "Selectable configuration libraries need a public paginated list action with exact record lookup",
        ));
    }
    Ok(())
}
pub(super) fn record<'a>(c: &'a ConfigurationContract, fields: &'a Value) -> Result<&'a str> {
    if let Some(inputs) = &c.inputs {
        inputs.arguments(fields)?;
        if let Some(name) = &inputs.record_field {
            return fields[name]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 100)
                .ok_or(bad("Configuration library record required"));
        }
    }
    Ok(&c.record)
}
pub(super) fn arguments(c: &ConfigurationContract, fields: &Value) -> Result<Vec<i64>> {
    if let Some(inputs) = &c.inputs {
        return inputs.arguments(fields);
    }
    let value = fields[&c.input_field]
        .as_str()
        .filter(|s| s.len() <= 2000 && !s.chars().any(char::is_control))
        .ok_or(bad("Printable configuration field required"))?;
    if fields.as_object().is_none_or(|o| o.len() != 1) {
        return Err(bad("Only declared configuration input is accepted"));
    }
    Ok(vec![value.chars().count() as i64])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package() -> Manifest {
        serde_json::from_str(include_str!(
            "../../extensions/apps/furniture-configurator/manifest.json"
        ))
        .unwrap()
    }
    #[test]
    fn library_and_dimensions_share_real_field_validation_and_legacy_digests() {
        let m = package();
        super::super::validate(&m).unwrap();
        let c = m.configuration.unwrap();
        let fields = json!({"material":"oak","width_cm":260,"depth_cm":80});
        assert_eq!(arguments(&c, &fields).unwrap(), vec![260, 80]);
        assert_eq!(record(&c, &fields).unwrap(), "oak");
        let guest = Sandbox::new(&c.wasm_source).unwrap();
        assert_eq!(
            guest
                .contribution_args(0, &arguments(&c, &fields).unwrap())
                .unwrap(),
            4800
        );
        assert_eq!(guest.contribution_args(2000, &[240, 80]).unwrap(), 2000);
        for invalid in [
            json!({"material":"oak","width_cm":401,"depth_cm":80}),
            json!({"material":"oak","width_cm":"260","depth_cm":80}),
            json!({"material":"oak","width_cm":260,"depth_cm":80,"price":0}),
            json!({"material":"oak","width_cm":260}),
        ] {
            assert!(arguments(&c, &invalid).is_err());
        }
        let legacy: Manifest = serde_json::from_str(include_str!(
            "../../extensions/apps/gift-message/manifest.json"
        ))
        .unwrap();
        assert!(json!(legacy)["configuration"].get("inputs").is_none());
        assert!(json!(legacy)["configuration"].get("currency").is_none());
        assert_eq!(
            arguments(
                legacy.configuration.as_ref().unwrap(),
                &json!({"message":"Hello"})
            )
            .unwrap(),
            vec![5]
        );
    }
    #[test]
    fn structured_contract_cannot_select_private_data_or_undeclared_arguments() {
        let mut omitted = json!(package());
        omitted["configuration"]["inputs"]["examples"][0]
            .as_object_mut()
            .unwrap()
            .remove("expectedFeeMinor");
        assert!(serde_json::from_value::<Manifest>(omitted).is_err());
        let mut m = package();
        m.entities[0].public_read = false;
        assert!(validate(&m).is_err());
        let mut m = package();
        m.configuration
            .as_mut()
            .unwrap()
            .inputs
            .as_mut()
            .unwrap()
            .arguments
            .push("missing".into());
        assert!(validate(&m).is_err());
        let mut m = package();
        m.configuration.as_mut().unwrap().currency = Some("BAD".into());
        assert!(validate(&m).is_err());
    }
}
