//! Settings round trips, partial updates and browser-safe secret projections without identity or encryption fixtures.
use serde_json::{Value, json};
use veyndo_app_model::settings::{self, Setting};
fn fields() -> Vec<Setting> {
    serde_json::from_value(json!([
     {"key":"token","label":{"de-DE":"Zugang","it-IT":"Accesso"},"kind":"secret","required":true},
     {"key":"title","label":{"de-DE":"Titel"},"kind":"string"},
     {"key":"enabled","label":{"de-DE":"Aktiv"},"kind":"boolean"},
     {"key":"limit","label":{"de-DE":"Limit"},"kind":"integer"}
    ]))
    .unwrap()
}
#[test]
fn partial_update_preserves_secret_and_distinguishes_false_zero_empty_and_null() {
    let f = fields();
    let current = json!({"token":"synthetic-test-value","title":"old","removed":"discard"});
    let encoded =
        settings::prepare(&f, &current, &json!({"title":"","enabled":false,"limit":0})).unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        value,
        json!({"token":"synthetic-test-value","title":"","enabled":false,"limit":0})
    );
    assert_eq!(
        settings::visible(&f, &value),
        json!({"title":"","enabled":false,"limit":0})
    );
    assert_eq!(
        settings::configured(&f, &value),
        vec!["token", "title", "enabled", "limit"]
    );
    let value: Value =
        serde_json::from_str(&settings::prepare(&f, &value, &json!({"title":null})).unwrap())
            .unwrap();
    assert!(!settings::configured(&f, &value).contains(&"title".into()));
    assert_eq!(settings::visible(&f, &value)["title"], Value::Null);
    assert_eq!(current["title"], "old");
}
#[test]
fn required_secret_cannot_be_deleted_or_cleared() {
    let f = fields();
    for input in [json!({}), json!({"token":null}), json!({"token":""})] {
        assert!(settings::prepare(&f, &json!({}), &input).is_err());
    }
    let current = json!({"token":"synthetic-test-value"});
    for input in [json!({"token":null}), json!({"token":""})] {
        assert!(settings::prepare(&f, &current, &input).is_err());
    }
    assert!(settings::prepare(&f, &Value::Null, &json!({})).is_err());
    assert!(settings::prepare(&f, &current, &json!([])).is_err());
}
#[test]
fn wrong_types_undeclared_keys_controls_and_oversized_strings_are_rejected() {
    let f = fields();
    let current = json!({"token":"synthetic-test-value"});
    for input in [
        json!({"unknown":"x"}),
        json!({"enabled":"false"}),
        json!({"limit":1.2}),
        json!({"limit":9223372036854775808_u64}),
        json!({"title":{}}),
        json!({"title":"a\nb"}),
        json!({"title":"x".repeat(4097)}),
    ] {
        assert!(settings::prepare(&f, &current, &input).is_err(), "{input}");
    }
    let many: serde_json::Map<String, Value> =
        (0..17).map(|i| (format!("key{i}"), json!("x"))).collect();
    assert!(settings::prepare(&f, &current, &Value::Object(many)).is_err());
}
#[test]
fn total_encoded_size_is_bounded_even_when_individual_values_fit() {
    let mut f = fields();
    for i in 0..4 {
        let mut n = f[1].clone();
        n.key = format!("text{i}");
        f.push(n);
    }
    let mut input = json!({"token":"synthetic-test-value"});
    for i in 0..4 {
        input[format!("text{i}")] = json!("x".repeat(4096));
    }
    assert!(settings::prepare(&f, &json!({}), &input).is_err());
    input["text3"] = json!("ok");
    settings::prepare(&f, &json!({}), &input).unwrap();
}
#[test]
fn settings_wire_is_strict_and_localized_declarations_keep_original_defaults() {
    let raw = json!({"key":"title","label":{"de-DE":"Titel","it-IT":"Titolo"},"kind":"string","required":false});
    let setting: Setting = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(setting).unwrap(), raw);
    let mut bad = raw.clone();
    bad["sql"] = json!("x");
    assert!(serde_json::from_value::<Setting>(bad).is_err());
    for key in ["Bad key", "id;drop"] {
        let mut f = fields();
        f[0].key = key.into();
        assert!(settings::validate(&f).is_err());
    }
    let mut f = fields();
    f.push(f[0].clone());
    assert!(settings::validate(&f).is_err());
    let mut f = fields();
    f[0].label.clear();
    assert!(settings::validate(&f).is_err());
    let mut f = fields();
    f[0].kind = "sql".into();
    assert!(settings::validate(&f).is_err());
    let mut f = fields();
    f[0].label.insert("de-DE".into(), "x".repeat(101));
    assert!(settings::validate(&f).is_err());
    let mut f = fields();
    f[0].label = (0..101).map(|i| (format!("lang{i}"), "x".into())).collect();
    assert!(settings::validate(&f).is_err());
    let mut f = fields();
    for i in 0..13 {
        let mut n = f[1].clone();
        n.key = format!("field{i}");
        f.push(n);
    }
    assert!(settings::validate(&f).is_err());
}
#[test]
fn removed_and_undeclared_secret_values_never_reach_runtime_or_browser() {
    let mut f = fields();
    f.retain(|s| s.key != "token");
    let stored = json!({"token":"synthetic-test-value","title":"Visible","other":"hidden"});
    assert_eq!(settings::declared(&f, &stored), json!({"title":"Visible"}));
    assert_eq!(settings::visible(&f, &stored), json!({"title":"Visible"}));
    assert_eq!(settings::configured(&f, &stored), vec!["title"]);
}
