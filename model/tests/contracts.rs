//! Independent model admission, host-policy boundaries, wire stability, typed values and language cases.
use serde_json::{Value, json};
use veyndo_app_model::{
    Entity, Kind,
    declarations::{self, ReferencePolicy},
    fields::{self, ValuePolicy},
    language, lifecycle, publication, schema,
};
struct Host {
    allow_external: bool,
}
impl ReferencePolicy for Host {
    fn admitted(&self, product: bool, private: bool) -> bool {
        product || private
    }
}
impl ValuePolicy for Host {
    fn locale(&self, key: &str) -> bool {
        ["fr-FR", "en-GB", "de-DE", "es-ES", "it-IT"].contains(&key)
    }
    fn money(&self, _value: &Value) -> bool {
        self.allow_external
    }
    fn richtext(&self, _value: &Value) -> bool {
        self.allow_external
    }
    fn relations(&self, _value: &Value) -> bool {
        self.allow_external
    }
}
fn entity(value: Value) -> Entity {
    serde_json::from_value(value).unwrap()
}
fn simple() -> Entity {
    entity(json!({"name":"items","fields":[{"name":"title","kind":"string","required":true}]}))
}
#[test]
fn strict_wire_contract_preserves_legacy_absence_and_rejects_unknowns() {
    let mut raw = json!({"name":"items","fields":[{"name":"title","kind":"string","required":true,"indexed":false,"references":null}],"publicRead":false});
    let parsed = entity(raw.clone());
    assert_eq!(serde_json::to_value(parsed).unwrap(), raw);
    raw["sql"] = json!("DROP TABLE products");
    assert!(serde_json::from_value::<Entity>(raw).is_err());
    let mut raw = serde_json::to_value(simple()).unwrap();
    raw["fields"][0]["permission"] = json!("admin");
    assert!(serde_json::from_value::<Entity>(raw).is_err());
}
#[test]
fn model_declarations_reject_invalid_types_duplicate_names_and_private_targets() {
    let host = Host {
        allow_external: false,
    };
    declarations::validate(&[simple()], &host).unwrap();
    for field in [
        json!({"name":"id","kind":"string"}),
        json!({"name":"text","kind":"sql"}),
        json!({"name":"text","kind":"string","translatable":true,"indexed":true}),
        json!({"name":"text","kind":"relations"}),
    ] {
        let e = entity(json!({"name":"items","fields":[field]}));
        assert!(declarations::validate(&[e], &host).is_err());
    }
    assert!(declarations::validate(&[simple(), simple()], &host).is_err());
    let target = entity(json!({"name":"private_items","fields":[{"name":"name","kind":"string"}]}));
    let source = entity(
        json!({"name":"public_items","publicRead":true,"fields":[{"name":"related","kind":"string","references":"private_items"}]}),
    );
    assert!(declarations::validate(&[source, target], &host).is_err());
    for id in ["1abc", "x;drop", "tenant--", "é", ""] {
        assert!(!declarations::identifier(id));
    }
}
#[test]
fn native_reference_policy_is_required_and_receives_original_facts() {
    let host = Host {
        allow_external: false,
    };
    let customer = entity(
        json!({"name":"items","publicRead":true,"fields":[{"name":"customer","kind":"string","coreReference":"customer","required":true,"indexed":true}]}),
    );
    assert!(declarations::validate(&[customer], &host).is_err());
    let product = entity(
        json!({"name":"items","publicRead":true,"fields":[{"name":"product","kind":"string","coreReference":"product","required":true,"indexed":true}]}),
    );
    declarations::validate(std::slice::from_ref(&product), &host).unwrap();
    struct Deny;
    impl ReferencePolicy for Deny {
        fn admitted(&self, _: bool, _: bool) -> bool {
            false
        }
    }
    assert!(declarations::validate(&[product], &Deny).is_err());
}
#[test]
fn exact_values_reject_coercion_unknowns_bad_dates_and_external_policy_denial() {
    let host = Host {
        allow_external: false,
    };
    fields::validate(&simple(), &json!({"title":"ok"}), &host).unwrap();
    for value in [
        json!({}),
        json!({"title":1}),
        json!({"title":"ok","tenant":"foreign"}),
    ] {
        assert!(fields::validate(&simple(), &value, &host).is_err());
    }
    for (kind, bad, good) in [
        ("integer", json!("2"), json!(2)),
        ("date", json!("2026-02-29"), json!("2024-02-29")),
        ("decimal", json!(0.1), json!("123456789012345678.12345678")),
        (
            "datetime",
            json!("2026-11-05"),
            json!("2026-11-05T12:00:00Z"),
        ),
    ] {
        let e = entity(json!({"name":"values","fields":[{"name":"value","kind":kind}]}));
        assert!(fields::validate(&e, &json!({"value":bad}), &host).is_err());
        fields::validate(&e, &json!({"value":good}), &host).unwrap();
    }
    for (kind, value) in [
        (
            "money",
            json!({"minor":123,"currency":{"code":"JPY","scale":0}}),
        ),
        ("richtext", json!({"fr-FR":[]})),
        ("relations", json!(["owned"])),
    ] {
        let e = entity(json!({"name":"external","fields":[{"name":"value","kind":kind}]}));
        assert!(fields::validate(&e, &json!({"value":value}), &host).is_err());
        fields::validate(
            &e,
            &json!({"value":value}),
            &Host {
                allow_external: true,
            },
        )
        .unwrap();
    }
}
#[test]
fn french_main_fifth_language_nulls_and_enabled_language_rules_survive() {
    let mut e = simple();
    e.fields[0].translatable = true;
    let host = Host {
        allow_external: false,
    };
    let locales = ["fr-FR", "en-GB", "de-DE", "es-ES", "it-IT"].map(str::to_owned);
    let value = json!({"title":{"fr-FR":"Lampe","it-IT":null}});
    fields::validate(&e, &value, &host).unwrap();
    language::validate(&e, &value, &locales, "fr-FR").unwrap();
    assert!(
        language::validate(&e, &json!({"title":{"it-IT":"Lampada"}}), &locales, "fr-FR").is_err()
    );
    assert!(
        language::validate(
            &e,
            &json!({"title":{"fr-FR":"Lampe","nl-NL":"Lamp"}}),
            &locales,
            "fr-FR"
        )
        .is_err()
    );
    fields::validate(&e, &json!({"title":{"fr-FR":""}}), &host).unwrap();
}
#[test]
fn publication_and_workflow_contracts_do_not_admit_malformed_states() {
    let e = entity(
        json!({"name":"cases","fields":[{"name":"status","kind":"string","required":true,"choices":[{"value":"new","label":{"en":"New"}},{"value":"done","label":{"en":"Done"}}]},{"name":"order_id","kind":"string"}],"workflow":{"field":"status","initial":"new","immutableFields":["order_id"],"transitions":[{"from":"new","to":"done"}]}}),
    );
    declarations::validate(
        std::slice::from_ref(&e),
        &Host {
            allow_external: false,
        },
    )
    .unwrap();
    let new = json!({"status":"new","order_id":"owned"});
    let done = json!({"status":"done","order_id":"owned"});
    lifecycle::change(&e, None, &new).unwrap();
    lifecycle::change(&e, Some(&new), &done).unwrap();
    assert_eq!(
        lifecycle::change(&e, Some(&done), &new).unwrap_err().kind,
        Kind::Conflict
    );
    assert!(
        lifecycle::change(
            &e,
            Some(&new),
            &json!({"status":"done","order_id":"foreign"})
        )
        .is_err()
    );
    let mut e = simple();
    e.publication = Some(
        serde_json::from_value(
            json!({"statusField":"title","channelField":"title","startsAt":"missing"}),
        )
        .unwrap(),
    );
    assert!(publication::validate(&e).is_err());
}
#[test]
fn schema_errors_preserve_pointers_and_closed_contracts() {
    let s = json!({"type":"object","properties":{"a/b~c":{"type":"integer"}},"required":["a/b~c"],"additionalProperties":false});
    schema::validate_contract(&s).unwrap();
    let e = schema::validate(&s, &json!({"a/b~c":"wrong"})).unwrap_err();
    assert_eq!(e.kind, Kind::Validation);
    assert_eq!(e.pointer.as_deref(), Some("/a~1b~0c"));
    assert!(schema::validate_contract(&json!({"$ref":"https://external.invalid/schema"})).is_err());
}
