//! Independent graph mappings retain selected fields/current revision, namespaces, bounded projections and reference-only edges.
use serde_json::json;
use veyndo_app_model::{intelligence, ontology};
#[path = "support/graph.rs"]
mod support;
use support::Fixture;
#[test]
fn selected_graph_properties_and_reference_edges_keep_version_revision_and_labels() {
    let mut f = Fixture::new();
    f.ai.ontology[0]
        .relations
        .insert("product_id".into(), "applies_to".into());
    intelligence::validate(Some(&f.ai), &f.actions, &f.entities).unwrap();
    ontology::validate(&f.context()).unwrap();
    let rows = json!([{"id":"r1","revision":7,"product_id":"mug","care":{"de-DE":"Handwäsche","it-IT":"A mano"},"internal":"hidden"}]);
    let graph = ontology::project("tenant_one", &f.context(), &f.entities[0], &rows).unwrap();
    assert_eq!(
        graph["nodes"][0]["source"],
        json!({"tenant":"tenant_one","app":"care_graph","version":"1.0.0","entity":"guides","recordId":"r1","revision":7})
    );
    assert_eq!(graph["nodes"][0]["type"], "app.care_graph.care_advice");
    assert_eq!(graph["nodes"][0]["label"]["it-IT"], "Cura");
    assert!(graph["nodes"][0]["properties"].get("internal").is_none());
    assert_eq!(
        graph["edges"][0],
        json!({"type":"app.care_graph.applies_to","source":"app.care_graph.guides:r1","field":"product_id","targetType":"product","targetId":"mug","tenant":"tenant_one"})
    );
    assert_eq!(graph["visibility"], "merchant-private");
    assert_eq!(
        graph["assurance"],
        "native-app-records-not-confirmed-product-claims"
    );
    let changed = json!([{"id":"r1","revision":8,"product_id":"mug","care":"Updated"}]);
    let graph = ontology::project("tenant_one", &f.context(), &f.entities[0], &changed).unwrap();
    assert_eq!(graph["nodes"][0]["source"]["revision"], 8);
    assert_eq!(graph["nodes"][0]["properties"]["care"], "Updated");
}
#[test]
fn graph_declarations_cannot_select_undeclared_fields_or_hijack_native_namespaces() {
    for change in 0..12 {
        let mut f = Fixture::new();
        match change {
            0 => f.ai.ontology[0].fields = vec!["invented".into()],
            1 => f.ai.ontology[0].node_type = "product.property".into(),
            2 => f.ai.entities.clear(),
            3 => f.ai.ontology[0].label.clear(),
            4 => f.ai.ontology[0].fields = vec!["care".into(), "care".into()],
            5 => f.permissions.clear(),
            6 => {
                f.ai.ontology[0]
                    .relations
                    .insert("care".into(), "applies".into());
            }
            7 => {
                f.ai.ontology[0]
                    .relations
                    .insert("product_id".into(), "core.hijack".into());
            }
            8 => f.actions.clear(),
            9 => f.ai.ontology[0].entity = "missing".into(),
            10 => {
                f.ai.ontology[0].label.insert("de-DE".into(), " ".into());
            }
            _ => {
                f.ai.ontology[0].fields = vec!["care".into()];
                f.ai.ontology[0]
                    .relations
                    .insert("product_id".into(), "applies".into());
            }
        };
        assert!(ontology::validate(&f.context()).is_err(), "change {change}");
    }
    let mut f = Fixture::new();
    f.ai.ontology.push(f.ai.ontology[0].clone());
    assert!(ontology::validate(&f.context()).is_err());
    let mut f = Fixture::new();
    f.ai.ontology[0].fields.clear();
    assert!(ontology::validate(&f.context()).is_err());
    let mut f = Fixture::new();
    f.ai.ontology[0]
        .label
        .insert("de-DE".into(), "x".repeat(121));
    assert!(ontology::validate(&f.context()).is_err());
    let mut f = Fixture::new();
    f.ai.ontology[0].label = (0..101)
        .map(|i| (format!("locale{i}"), "x".into()))
        .collect();
    assert!(ontology::validate(&f.context()).is_err());
    let mut f = Fixture::new();
    f.ai.ontology = vec![f.ai.ontology[0].clone(); 5];
    assert!(ontology::validate(&f.context()).is_err());
}
#[test]
fn row_node_byte_and_relation_limits_bound_actual_projection() {
    let f = Fixture::new();
    let rows: Vec<_> = (0..30)
        .map(|i| json!({"id":format!("r{i}"),"revision":1,"product_id":"mug","care":"Short"}))
        .collect();
    let graph = ontology::project("tenant", &f.context(), &f.entities[0], &json!(rows)).unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 24);
    assert_eq!(graph["omittedRecords"], 6);
    let huge = json!([{"id":"r1","revision":1,"product_id":"mug","care":"x".repeat(20000)}]);
    let graph = ontology::project("tenant", &f.context(), &f.entities[0], &huge).unwrap();
    assert_eq!(graph["nodes"], json!([]));
    assert_eq!(graph["omittedRecords"], 1);
    assert!(ontology::project("tenant", &f.context(), &f.entities[0], &json!({})).is_none());
    assert!(
        ontology::project(
            "tenant",
            &f.context(),
            &f.entities[0],
            &json!([{"revision":1}])
        )
        .is_none()
    );
}
#[test]
fn app_reference_edges_target_declared_namespaced_types_without_reading_targets() {
    let mut f = Fixture::new();
    let mut next = f.entities[0].clone();
    next.name = "notes".into();
    next.fields=serde_json::from_value(json!([{"name":"guides","kind":"relations","references":"guides"},{"name":"body","kind":"string"}])).unwrap();
    f.entities.push(next);
    let mapping:ontology::Mapping=serde_json::from_value(json!({"entity":"notes","nodeType":"care_note","label":{"de-DE":"Notiz"},"fields":["guides","body"],"relations":{"guides":"explains"}})).unwrap();
    f.ai.entities.push("notes".into());
    f.ai.ontology.push(mapping);
    let mut action = f.actions[0].clone();
    action.name = "list_notes".into();
    action.entity = Some("notes".into());
    f.actions.push(action);
    ontology::validate(&f.context()).unwrap();
    let ids: Vec<_> = (0..105).map(|_| json!("g")).collect();
    let rows = json!([{"id":"n","revision":2,"guides":ids,"body":"Read"}]);
    let mut context = f.context();
    context.id = "a";
    let graph = ontology::project("t", &context, &f.entities[1], &rows).unwrap();
    assert_eq!(graph["edges"].as_array().unwrap().len(), 100);
    assert_eq!(graph["edges"][0]["type"], "app.a.explains");
    assert_eq!(graph["edges"][0]["targetType"], "app.a.care_advice");
    assert!(graph["nodes"][0]["properties"].get("care").is_none());
}
#[test]
fn bindings_infer_graph_metadata_but_ordinary_legacy_models_do_not() {
    let mut f = Fixture::new();
    f.ai.ontology.clear();
    assert!(ontology::project("tenant", &f.context(), &f.entities[0], &json!([])).is_none());
    assert!(
        serde_json::to_value(&f.ai)
            .unwrap()
            .get("ontology")
            .is_none()
    );
    f.entities[0].extends = Some(
        serde_json::from_value(json!({"version":1,"object":"product","field":"product_id"}))
            .unwrap(),
    );
    let graph = ontology::project(
        "tenant",
        &f.context(),
        &f.entities[0],
        &json!([{"id":"one","revision":1,"product_id":"mug","care":"Read"}]),
    )
    .unwrap();
    assert_eq!(graph["nodes"][0]["type"], "app.care_graph.guides");
    assert_eq!(graph["edges"][0]["targetType"], "product");
    f.entities[0].public_read = true;
    let graph = ontology::project("tenant", &f.context(), &f.entities[0], &json!([])).unwrap();
    assert_eq!(graph["visibility"], "declared-public-records");
}
#[test]
fn intelligence_tools_entities_and_wire_contracts_remain_closed_metadata() {
    let f = Fixture::new();
    intelligence::validate(Some(&f.ai), &f.actions, &f.entities).unwrap();
    for change in 0..5 {
        let mut f = Fixture::new();
        match change {
            0 => f.ai.tools.push("missing".into()),
            1 => f.ai.entities.push("missing".into()),
            2 => f.ai.tools = vec!["list_guides".into(); 25],
            3 => f.ai.entities = vec!["guides".into(); 13],
            _ => {
                f.ai.description.insert("de-DE".into(), "x".repeat(1001));
            }
        };
        assert!(intelligence::validate(Some(&f.ai), &f.actions, &f.entities).is_err());
    }
    let mut raw = serde_json::to_value(&f.ai).unwrap();
    raw["permissions"] = json!(["admin"]);
    assert!(serde_json::from_value::<intelligence::Contract>(raw).is_err());
    let mut raw = serde_json::to_value(&f.ai.ontology[0]).unwrap();
    raw["sql"] = json!("DROP TABLE products");
    assert!(serde_json::from_value::<ontology::Mapping>(raw).is_err());
    let mut mapping = f.ai.ontology[0].clone();
    mapping.relations.clear();
    assert!(
        serde_json::to_value(mapping)
            .unwrap()
            .get("relations")
            .is_none()
    );
}
