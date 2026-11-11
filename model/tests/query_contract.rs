//! Native list discovery names only supported indexes and preserves authored action restrictions.
use serde_json::json;
use veyndo_app_model::{Entity, query_contract::list_input_schema, schema::validate_value};

#[test]
fn generic_filters_expose_actual_fields_types_choices_and_labels() {
    let entity: Entity = serde_json::from_value(json!({"name":"notes","fields":[
        {"name":"product_id","kind":"string","indexed":true,"label":{"en":"Product","de":"Produkt"}},
        {"name":"segment","kind":"string","indexed":true,"choices":[{"value":"standard","label":{}},{"value":"priority","label":{}}]},
        {"name":"count","kind":"integer","indexed":true},
        {"name":"active","kind":"boolean","indexed":true},
        {"name":"title","kind":"string","indexed":true,"translatable":true},
        {"name":"body","kind":"string"},
        {"name":"payload","kind":"json","indexed":true}
    ]})).unwrap();
    let declared = json!({"type":"object","properties":{"filter":{"type":"object"},"limit":{"type":"integer","maximum":7}},"additionalProperties":false});
    let schema = list_input_schema(&entity, &declared);
    veyndo_app_model::schema::validate_contract(&schema).unwrap();
    assert_eq!(schema["properties"]["limit"]["maximum"], 7);
    let properties = &schema["properties"]["filter"]["properties"];
    assert_eq!(properties.as_object().unwrap().len(), 4);
    assert_eq!(properties["product_id"]["description"], "Product / Produkt");
    assert_eq!(
        properties["segment"]["enum"],
        json!(["standard", "priority"])
    );
    validate_value(
        &schema,
        &json!({"filter":{"product_id":"mug","segment":"priority","active":true,"count":1}}),
    )
    .unwrap();
    for value in [
        json!({"filter":{"priority":true}}),
        json!({"filter":{"title":"a"}}),
        json!({"filter":{"count":"1"}}),
        json!({"filter":{"segment":"invented"}}),
        json!({"limit":8}),
    ] {
        assert!(validate_value(&schema, &value).is_err(), "{value}");
    }
    let authored = json!({"properties":{"filter":{"type":"object","properties":{"segment":{"enum":["priority"]}},"additionalProperties":false}}});
    assert_eq!(list_input_schema(&entity, &authored), authored);
    let forbidden = json!({"properties":{"filter":{"type":"object","additionalProperties":false}}});
    assert_eq!(list_input_schema(&entity, &forbidden), forbidden);
    for authored in [
        json!({"properties":{"filter":{"type":"string"}}}),
        json!({"properties":{"filter":{"type":"object","additionalProperties":{"type":"integer"}}}}),
    ] {
        assert_eq!(list_input_schema(&entity, &authored), authored);
    }
    assert_eq!(list_input_schema(&entity, &json!({})), json!({}));
}
