//! Discoverable native list filters from declared indexed fields; no storage or actor authority.
use crate::Entity;
use serde_json::{Map, Value, json};

/// Refine a generic native-list filter without replacing an app-authored schema.
/// Dispatch must still authorize the actor and validate the actual storage query.
pub fn list_input_schema(entity: &Entity, declared: &Value) -> Value {
    let mut schema = declared.clone();
    let Some(filter) = schema
        .pointer_mut("/properties/filter")
        .and_then(Value::as_object_mut)
    else {
        return schema;
    };
    if filter.contains_key("properties")
        || filter.get("type").is_some_and(|kind| kind != "object")
        || filter
            .get("additionalProperties")
            .is_some_and(|value| value != true)
    {
        return schema;
    }
    let mut properties = Map::new();
    for field in &entity.fields {
        if !field.indexed
            || field.translatable
            || !["string", "integer", "boolean"].contains(&field.kind.as_str())
        {
            continue;
        }
        let mut property = json!({"type":field.kind});
        let mut labels = field.label.values().cloned().collect::<Vec<_>>();
        labels.sort();
        labels.dedup();
        property["description"] = json!(labels.join(" / "));
        if field.kind == "string" {
            property["maxLength"] = json!(2000);
            if !field.choices.is_empty() {
                property["enum"] =
                    json!(field.choices.iter().map(|c| &c.value).collect::<Vec<_>>());
            }
        }
        properties.insert(field.name.clone(), property);
    }
    filter.insert("type".into(), json!("object"));
    filter.insert("properties".into(), json!(properties));
    filter.insert("additionalProperties".into(), json!(false));
    // Keep the supported schema grammar; storage enforces count and UTF-8 byte budgets.
    let description = filter
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    filter.insert("description".into(), json!(format!("{description} Use at most four indexed equality filters. String values allow at most 2000 UTF-8 bytes.")));
    schema
}
