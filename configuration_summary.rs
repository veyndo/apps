//! Immutable public configuration labels accompany accepted inputs; no current package lookup is needed on a receipt.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Snapshot {
    pub title: HashMap<String, String>,
    pub main_locale: String,
    pub fields: Vec<DisplayField>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DisplayField {
    pub label: HashMap<String, String>,
    pub value: DisplayValue,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum DisplayValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Localized(HashMap<String, String>),
}

pub(crate) fn snapshot(
    manifest: &Manifest,
    fields: &Value,
    public_rule: Option<&Value>,
    main_locale: String,
) -> Option<Snapshot> {
    let contract = manifest.configuration.as_ref()?;
    let mut rows = vec![];
    if let Some(inputs) = &contract.inputs {
        for field in &inputs.fields {
            let Some(raw) = fields.get(&field.name) else {
                continue;
            };
            let reference_name = (inputs.record_field.as_ref() == Some(&field.name))
                .then(|| {
                    manifest
                        .entities
                        .iter()
                        .find(|e| e.name == contract.entity && e.public_read)?
                        .fields
                        .iter()
                        .find(|f| f.translatable && f.kind == "string")
                        .and_then(|name| public_rule?.get(&name.name))
                })
                .flatten();
            let choice = field
                .choices
                .iter()
                .find(|choice| Some(choice.value.as_str()) == raw.as_str());
            let value = if let Some(choice) = choice {
                Some(DisplayValue::Localized(choice.label.clone()))
            } else {
                serde_json::from_value(reference_name.unwrap_or(raw).clone()).ok()
            };
            if let Some(value) = value {
                rows.push(DisplayField {
                    label: field.label.clone(),
                    value,
                });
            }
        }
    } else if let Some(value) = fields.get(&contract.input_field) {
        rows.push(DisplayField {
            label: contract.label.clone(),
            value: serde_json::from_value(value.clone()).ok()?,
        });
    }
    Some(Snapshot {
        title: contract.label.clone(),
        main_locale,
        fields: rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(json!({
          "id":"sample", "version":"1.0.0", "coreApi":"1", "runtime":"declarative",
          "name":{"it-IT":"Campione"}, "permissions":[],
          "entities":[{"name":"choices","publicRead":true,"fields":[
            {"name":"name","kind":"string","translatable":true},
            {"name":"internal","kind":"string"}]}],
          "configuration":{"entity":"choices","record":"one","priceField":"fee",
            "inputField":"size","wasmSource":"","defaultFields":{},
            "label":{"it-IT":"Scelta"},"hint":{},
            "inputs":{"recordField":"choice","arguments":["size"],"fields":[
              {"name":"choice","kind":"string","label":{"it-IT":"Materiale"}},
              {"name":"size","kind":"integer","label":{"it-IT":"Misura"}},
              {"name":"enabled","kind":"boolean","label":{"it-IT":"Attivo"}},
              {"name":"style","kind":"string","label":{"it-IT":"Stile"},
               "choices":[{"value":"plain","label":{"it-IT":"Semplice"}}]}]}}
        }))
        .unwrap()
    }
    #[test]
    fn freezes_declared_labels_and_public_reference_without_extra_data() {
        let mut m = manifest();
        let fields =
            json!({"choice":"one","size":260,"enabled":false,"style":"plain","secret":"never"});
        let rule = json!({"name":{"it-IT":"Rovere"},"internal":"never"});
        let result = snapshot(&m, &fields, Some(&rule), "it-IT".into()).unwrap();
        m.configuration
            .as_mut()
            .unwrap()
            .label
            .insert("it-IT".into(), "Changed".into());
        let value = json!(result);
        assert_eq!(value["mainLocale"], "it-IT");
        assert_eq!(value["title"]["it-IT"], "Scelta");
        assert_eq!(value["fields"].as_array().unwrap().len(), 4);
        assert_eq!(value["fields"][0]["value"]["it-IT"], "Rovere");
        assert_eq!(value["fields"][1]["value"], 260);
        assert_eq!(value["fields"][2]["value"], false);
        assert_eq!(value["fields"][3]["value"]["it-IT"], "Semplice");
        assert!(!value.to_string().contains("never"));
    }
    #[test]
    fn private_library_cannot_supply_a_display_name() {
        let mut m = manifest();
        m.entities[0].public_read = false;
        let result = snapshot(
            &m,
            &json!({"choice":"one"}),
            Some(&json!({"name":{"it-IT":"private"}})),
            "it-IT".into(),
        )
        .unwrap();
        assert_eq!(json!(result)["fields"][0]["value"], "one");
    }
}
