//! Independent business contract grammar rejects field, library, authority, wire and frozen snapshot drift.
use serde::Deserialize;
use serde_json::{Value, json};
use veyndo_app_model::{
    Action, Entity, business_capability, commerce_hooks, configuration,
    declarations::ReferencePolicy, fields::ValuePolicy,
};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Fixture {
    entities: Vec<Entity>,
    actions: Vec<Action>,
    permissions: Vec<String>,
    #[serde(default)]
    configuration: Option<configuration::Contract>,
    #[serde(default)]
    commerce_hooks: Option<commerce_hooks::Contract>,
    #[serde(default)]
    capability: Option<business_capability::Contract>,
}
impl Fixture {
    fn validate(&self) -> veyndo_app_model::Result<()> {
        configuration::validate(
            &configuration::Context {
                entities: &self.entities,
                actions: &self.actions,
                configuration: self.configuration.as_ref(),
            },
            &Host,
        )?;
        commerce_hooks::validate(
            &commerce_hooks::Context {
                entities: &self.entities,
                permissions: &self.permissions,
                commerce_hooks: self.commerce_hooks.as_ref(),
            },
            |s| {
                s == fixture("snapshot")["commerceHooks"]["source"]
                    .as_str()
                    .unwrap()
            },
        )?;
        business_capability::validate(
            &business_capability::Context {
                entities: &self.entities,
                commerce_hooks: self.commerce_hooks.as_ref(),
                capability: self.capability.as_ref(),
            },
            &Host,
        )
    }
}
struct Host;
impl ValuePolicy for Host {
    fn locale(&self, k: &str) -> bool {
        ["en", "de", "en-GB", "de-DE"].contains(&k)
    }
    fn money(&self, _: &Value) -> bool {
        false
    }
    fn richtext(&self, _: &Value) -> bool {
        false
    }
    fn relations(&self, _: &Value) -> bool {
        false
    }
}
impl ReferencePolicy for Host {
    fn admitted(&self, product: bool, private: bool) -> bool {
        product || private
    }
}
impl configuration::Policy for Host {
    fn known_currency(&self, k: &str) -> bool {
        ["EUR", "JPY"].contains(&k)
    }
}
fn fixture(name: &str) -> Value {
    serde_json::from_str(if name == "furniture" {
        include_str!("fixtures/business-furniture.json")
    } else {
        include_str!("fixtures/business-snapshot.json")
    })
    .unwrap()
}
fn parse(v: Value) -> Fixture {
    serde_json::from_value(v).unwrap()
}
#[test]
fn original_business_contracts_compile_without_core() {
    for name in ["furniture", "snapshot"] {
        parse(fixture(name)).validate().unwrap();
    }
    let m = parse(fixture("furniture"));
    let c = m.configuration.unwrap();
    let fields = json!({"material":"oak","width_cm":260,"depth_cm":80});
    assert_eq!(
        configuration::arguments(&c, &fields, &Host).unwrap(),
        [260, 80]
    );
    assert_eq!(configuration::record(&c, &fields, &Host).unwrap(), "oak");
}
#[test]
fn configuration_schema_library_and_exact_arguments_are_not_coerced() {
    for mutation in 0..9 {
        let mut v = fixture("furniture");
        match mutation {
            0 => v["configuration"]["currency"] = json!("BAD"),
            1 => v["configuration"]["inputs"]["arguments"] = json!(["width_cm", "width_cm"]),
            2 => v["configuration"]["inputs"]["arguments"] = json!(["missing"]),
            3 => v["entities"][0]["publicRead"] = json!(false),
            4 => v["configuration"]["inputs"]["fields"][0]["translatable"] = json!(true),
            5 => v["configuration"]["inputs"]["fields"][0]["coreReference"] = json!("product"),
            6 => v["configuration"]["inputs"]["fields"][0]["required"] = json!(false),
            7 => v["configuration"]["inputs"]["examples"][0]["materialFeeMinor"] = json!(-1),
            _ => v["configuration"]["inputs"]["examples"][0]["expectedFeeMinor"] = json!(100000001),
        }
        assert!(parse(v).validate().is_err(), "mutation {mutation}");
    }
    let m = parse(fixture("furniture"));
    let c = m.configuration.unwrap();
    for fields in [
        json!({"material":"oak","width_cm":"260","depth_cm":80}),
        json!({"material":"oak","width_cm":260,"depth_cm":80,"price":0}),
        json!({"material":"oak","width_cm":401,"depth_cm":80}),
        json!({"material":"oak\n","width_cm":260,"depth_cm":80}),
    ] {
        assert!(configuration::arguments(&c, &fields, &Host).is_err());
    }
}
#[test]
fn expected_rejections_are_explicit_null_and_wire_properties_are_strict() {
    let mut v = fixture("furniture");
    v["configuration"]["inputs"]["examples"][0]
        .as_object_mut()
        .unwrap()
        .remove("expectedFeeMinor");
    assert!(serde_json::from_value::<Fixture>(v).is_err());
    let mut v = fixture("furniture");
    v["configuration"]["inputs"]["execute"] = json!("untrusted");
    assert!(serde_json::from_value::<Fixture>(v).is_err());
    let mut v = fixture("snapshot");
    v["capability"]["examples"][0]["snapshot"]["tenant"] = json!("foreign");
    assert!(serde_json::from_value::<Fixture>(v).is_err());
}
#[test]
fn source_admission_and_hook_scope_are_required() {
    let m = parse(fixture("snapshot"));
    assert!(
        commerce_hooks::validate(
            &commerce_hooks::Context {
                entities: &m.entities,
                permissions: &m.permissions,
                commerce_hooks: m.commerce_hooks.as_ref()
            },
            |_| false
        )
        .is_err()
    );
    for mutation in 0..5 {
        let mut v = fixture("snapshot");
        match mutation {
            0 => v["permissions"] = json!([]),
            1 => v["commerceHooks"]["hooks"] = json!(["price", "price"]),
            2 => v["commerceHooks"]["hooks"] = json!(["payment"]),
            3 => v["commerceHooks"]["records"] = json!([{"entity":"foreign","id":"other"}]),
            _ => v["commerceHooks"]["source"] = json!("x".repeat(32769)),
        }
        assert!(parse(v).validate().is_err());
    }
}
#[test]
fn frozen_snapshots_expected_money_and_context_stay_bounded() {
    for mutation in 0..9 {
        let mut v = fixture("snapshot");
        match mutation {
            0 => v["capability"]["version"] = json!(2),
            1 => v["capability"]["examples"][0]["hook"] = json!("payment"),
            2 => v["capability"]["examples"][0]["snapshot"]["subtotal"]["currency"] = json!("eur"),
            3 => v["capability"]["examples"][0]["snapshot"]["subtotal"]["minor"] = json!(-1),
            4 => {
                v["capability"]["examples"][0]["snapshot"]["records"] =
                    json!([{"entity":"foreign","id":"live","revision":1,"fields":{}}])
            }
            5 => v["capability"]["examples"][0]["context"] = json!({"large":"x".repeat(8193)}),
            6 => v["capability"]["examples"][0]["expected"]["adjustment"]["scale"] = json!(3),
            7 => v["capability"]["examples"][0]["expected"]["reasonCode"] = json!("bad.path"),
            _ => {
                v["capability"]["examples"][0]["expected"]["adjustment"]["minor"] = json!(100000001)
            }
        }
        assert!(parse(v).validate().is_err(), "mutation {mutation}");
    }
    let mut v = fixture("snapshot");
    v["capability"]["examples"][0]["hook"] = json!("undeclared");
    let err = parse(v).validate().unwrap_err();
    assert_eq!(err.pointer.as_deref(), Some("/capability/examples/0"));
}
#[test]
fn structured_boolean_exact_json_range_and_legacy_text_keep_their_wire_meaning() {
    let mut m = parse(fixture("furniture"));
    let c = m.configuration.as_mut().unwrap();
    let input = c.inputs.as_mut().unwrap();
    input.record_field = None;
    input.fields = vec![
        serde_json::from_value(
            json!({"name":"enabled","kind":"boolean","required":true,"label":{"en":"Enabled"}}),
        )
        .unwrap(),
    ];
    input.arguments = vec!["enabled".into()];
    input.examples.clear();
    m.validate().unwrap();
    let c = m.configuration.as_ref().unwrap();
    assert_eq!(
        configuration::arguments(c, &json!({"enabled":false}), &Host).unwrap(),
        [0]
    );
    assert_eq!(
        configuration::arguments(c, &json!({"enabled":true}), &Host).unwrap(),
        [1]
    );
    assert!(configuration::arguments(c, &json!({"enabled":"true"}), &Host).is_err());
    let mut m = parse(fixture("furniture"));
    let c = m.configuration.as_mut().unwrap();
    c.inputs
        .as_mut()
        .unwrap()
        .fields
        .iter_mut()
        .find(|f| f.name == "width_cm")
        .unwrap()
        .validation = None;
    assert!(
        configuration::arguments(
            c,
            &json!({"material":"oak","width_cm":9007199254740992_i64,"depth_cm":80}),
            &Host
        )
        .is_err()
    );
    let mut legacy = c.clone();
    legacy.inputs = None;
    legacy.currency = None;
    legacy.input_field = "message".into();
    assert_eq!(
        configuration::arguments(&legacy, &json!({"message":"Grüße"}), &Host).unwrap(),
        [5]
    );
    assert!(configuration::arguments(&legacy, &json!({"message":"Hi","amount":0}), &Host).is_err());
}
