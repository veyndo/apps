//! Adversarial schema cases exercise bounded inputs, nullable unions, escaped JSON pointers and output values.
use super::*;
#[test]
fn nullable_types_and_json_pointers_are_enforced() {
    let schema = json!({"type":"object","properties":{"a/b~c":{"type":["string","null"]}},"required":["a/b~c"],"additionalProperties":false});
    validate_contract(&schema).unwrap();
    for value in [json!({"a/b~c":null}), json!({"a/b~c":"fine"})] {
        validate(&schema, &value).unwrap();
    }
    for value in [json!({"a/b~c":true}), json!({})] {
        let error = validate(&schema, &value).unwrap_err();
        assert_eq!(error.problem()["pointer"], "/a~1b~0c");
    }
    assert!(validate_contract(&json!({"type":[]})).is_err());
    assert!(validate_contract(&json!({"type":["string","invalid"]})).is_err());
    assert!(validate_contract(&json!({"type":["array","null"]})).is_err());
    let nullable_array = json!({"type":["array","null"],"items":{"type":"integer"}});
    validate_contract(&nullable_array).unwrap();
    assert!(validate_value(&nullable_array, &json!(null)).is_ok());
    assert!(validate_value(&nullable_array, &json!([1, 2])).is_ok());
    assert!(validate_value(&nullable_array, &json!(["invalid"])).is_err());
}
#[test]
fn nested_types_choices_and_bounds_are_enforced() {
    let s = json!({"type":"object","additionalProperties":false,"properties":{"items":{"type":"array","maxItems":2,"items":{"type":"object","additionalProperties":false,"required":["price"],"properties":{"price":{"type":"number","minimum":0,"maximum":5},"name":{"type":"string","maxLength":3}}}}}});
    assert!(validate(&s, &json!({"items":[{"price":1.25,"name":"abc"}]})).is_ok());
    for value in [
        json!({"items":[{"price":"1"}]}),
        json!({"items":[{"price":-1}]}),
        json!({"items":[{"price":1,"secret":true}]}),
        json!({"items":[{"price":1,"name":"long"}]}),
        json!({"items":[{"price":1},{"price":1},{"price":1}]}),
    ] {
        assert!(validate(&s, &value).is_err());
    }
}
#[test]
fn unsupported_or_malformed_constraints_fail_closed() {
    for s in [
        json!({"type":"string","maxLength":"5"}),
        json!({"type":"number","minimum":5,"maximum":2}),
        json!({"type":"integer","exclusiveMinimum":0}),
        json!({"type":"object","required":["missing"]}),
        json!({"type":"array","items":{"type":"integer"},"uniqueItems":"yes"}),
    ] {
        assert!(validate_contract(&s).is_err(), "{s}");
    }
}
#[test]
fn open_managed_fields_remain_bounded() {
    let s = json!({"type":"object","properties":{"fields":{"type":"object"}}});
    let mut v = json!(0);
    for _ in 0..14 {
        v = json!({"nested":v});
    }
    assert!(validate(&s, &json!({"fields":v})).is_err());
    assert!(validate(&s, &json!({"fields":{"title":"x".repeat(8193)}})).is_err());
}
#[test]
fn native_rich_content_keeps_domain_depth_without_widening_app_actions() {
    let schema = json!({"type":"object","properties":{"product":{"type":"object"}}});
    let v = json!({"product":{"extra":{"richDescription":{"de":[{"type":"document","doc":{"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"Hello","marks":[{"type":"link","attrs":{"href":"https://example.test"}}]}]}]}}]}}}});
    // This is an already valid native product, not an arbitrary app envelope.
    validate_native(&schema, &v).unwrap();
    assert!(validate(&schema, &v).is_err());
    let mut deep = json!(0);
    for _ in 0..34 {
        deep = json!({"nested":deep});
    }
    assert!(validate_native(&schema, &json!({"product":deep})).is_err());
    assert!(validate_native(&schema, &json!({"product":{"text":"x".repeat(65536)}})).is_err());
}
