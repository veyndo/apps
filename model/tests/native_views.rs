//! Independent native view compiler tests exercise strict wire syntax, typed UI behavior, host policy and shared projections.
use serde::Deserialize;
use serde_json::{Value, json};
use veyndo_app_model::{
    Action, Entity,
    fields::ValuePolicy,
    surfaces::Surface,
    views::{self, Contract, NativeView, Policy},
};
#[derive(Deserialize, Clone)]
#[serde(deny_unknown_fields)]
struct Fixture {
    entities: Vec<Entity>,
    actions: Vec<Action>,
    surfaces: Vec<Surface>,
    views: Vec<NativeView>,
}
impl Fixture {
    fn contract(&self) -> Contract<'_> {
        Contract {
            entities: &self.entities,
            actions: &self.actions,
            surfaces: &self.surfaces,
            views: &self.views,
        }
    }
    fn validate(&self) -> veyndo_app_model::Result<()> {
        views::validate(&self.contract(), &Host)
    }
}
struct Host;
impl ValuePolicy for Host {
    fn locale(&self, key: &str) -> bool {
        ["en", "de", "en-GB", "de-DE"].contains(&key)
    }
    // This fixture host has no currency, rich-content or relation owner. It explicitly rejects those values.
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
impl Policy for Host {
    fn public_read(&self, writes: bool) -> bool {
        !writes
    }
}
fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/native_views.json")).unwrap()
}
fn behavior() -> Fixture {
    let mut m = fixture();
    let form = m.views[0]
        .blocks
        .iter()
        .find(|b| b.kind == "form")
        .unwrap()
        .id
        .clone();
    m.entities[0]
        .fields
        .push(serde_json::from_value(json!({"name":"quantity","kind":"integer"})).unwrap());
    m.views[0].blocks.push(serde_json::from_value(json!({"id":"qty","kind":"textbox","title":{"en":"Quantity"},"entity":"guides","dataField":"quantity","readAction":"list_guides"})).unwrap());
    let refresh = m.views[0].blocks[0].id.clone();
    m.views[0].blocks.push(serde_json::from_value(json!({"id":"save_button","kind":"button","title":{"en":"Save"},"handlers":{"click":[{"op":"validate","target":form},{"op":"call","action":"save_guides","input":{"kind":"record","block":form}},{"op":"refresh","target":refresh}]}})).unwrap());
    m
}
fn set(m: &mut Fixture, steps: Value) {
    m.views[0]
        .blocks
        .iter_mut()
        .find(|b| b.id == "save_button")
        .unwrap()
        .handlers = Some(serde_json::from_value(json!({"click":steps})).unwrap());
}
#[test]
fn original_views_and_behavior_compile_and_project_only_bound_metadata() {
    let m = behavior();
    m.validate().unwrap();
    let p = views::payload(&m.contract(), &m.surfaces[1]).unwrap();
    assert_eq!(p["entities"].as_array().unwrap().len(), 1);
    assert!(p["navigation"].get("manager").is_none());
    assert!(views::is_native(&m.contract(), &m.surfaces[0]));
    let mut unknown = m.surfaces[0].clone();
    unknown.ui_path = "native/missing".into();
    assert!(views::payload(&m.contract(), &unknown).is_none());
}
#[test]
fn bindings_preserve_exact_error_kind_and_pointer() {
    let mut m = fixture();
    m.views[0].blocks[0].read_action = Some("save_guides".into());
    let err = m.validate().unwrap_err();
    assert_eq!(err.kind, veyndo_app_model::Kind::Validation);
    assert_eq!(err.pointer.as_deref(), Some("/views/0/blocks/0/readAction"));
    m.views[0].blocks[0].read_action = None;
    assert_eq!(
        m.validate().unwrap_err().pointer.as_deref(),
        Some("/views/0/blocks/0/readAction")
    );
}
#[test]
fn public_writes_and_unlisted_actions_stay_denied() {
    for change in 0..5 {
        let mut m = fixture();
        match change {
            0 => m.views[1].blocks[0].inline_edit = true,
            1 => {
                m.views[1].blocks[0].kind = "form".into();
                m.views[1].blocks[0].write_action = Some("save_guides".into());
            }
            2 => m.surfaces[0].actions.clear(),
            3 => m.views[0].blocks[0].write_action = Some("save_guides".into()),
            _ => m.surfaces[0].ui_path = "native/unknown".into(),
        }
        assert!(m.validate().is_err(), "change {change}");
    }
    let mut m = behavior();
    m.surfaces[0].actions.retain(|a| a != "save_guides");
    assert!(m.validate().is_err());
}
#[test]
fn geometry_and_container_ownership_are_bounded() {
    for geometry in [
        json!({"x":11,"y":0,"w":2,"h":1}),
        json!({"x":65535,"y":0,"w":65535,"h":1}),
        json!({"x":0,"y":201,"w":1,"h":1}),
        json!({"x":0,"y":0,"w":0,"h":1}),
    ] {
        let mut m = fixture();
        m.views[0].blocks[0].geometry = Some(serde_json::from_value(geometry).unwrap());
        assert!(m.validate().is_err());
    }
    let mut m = behavior();
    let container =
        json!({"id":"frame","kind":"frame","title":{"en":"Group"},"childBlocks":["qty"]});
    m.views[0]
        .blocks
        .push(serde_json::from_value(container.clone()).unwrap());
    m.validate().unwrap();
    let mut second = container;
    second["id"] = json!("other");
    m.views[0]
        .blocks
        .push(serde_json::from_value(second).unwrap());
    assert!(m.validate().is_err());
}
#[test]
fn static_control_types_and_schema_literals_retain_native_rules() {
    let mut m = behavior();
    set(
        &mut m,
        json!([{"op":"set","target":"qty","value":{"kind":"literal","value":10}}]),
    );
    m.validate().unwrap();
    for value in [json!("10"), json!(0.1), json!(true), json!({"x":10})] {
        set(
            &mut m,
            json!([{"op":"set","target":"qty","value":{"kind":"literal","value":value}}]),
        );
        assert!(m.validate().is_err());
    }
    set(
        &mut m,
        json!([{"op":"call","action":"save_guides","input":{"kind":"literal","value":{"unknown":true}}}]),
    );
    assert!(m.validate().is_err());
    set(
        &mut m,
        json!([{"op":"if","left":{"kind":"value","block":"qty"},"compare":"gt","right":{"kind":"literal","value":"5"},"then":[]}]),
    );
    assert!(m.validate().is_err());
}
#[test]
fn literal_assignment_invokes_the_required_host_value_policy() {
    let mut m = behavior();
    let q = m.entities[0]
        .fields
        .iter_mut()
        .find(|f| f.name == "quantity")
        .unwrap();
    q.kind = "string".into();
    q.translatable = true;
    set(
        &mut m,
        json!([{"op":"set","target":"qty","value":{"kind":"literal","value":{"de-DE":"Zehn"}}}]),
    );
    m.validate().unwrap();
    set(
        &mut m,
        json!([{"op":"set","target":"qty","value":{"kind":"literal","value":{"untrusted":"Ten"}}}]),
    );
    assert!(m.validate().is_err());
}
#[test]
fn ui_syntax_rejects_unknown_fields_and_prototype_keys() {
    assert!(
        serde_json::from_value::<veyndo_app_model::ui_ast::Expression>(
            json!({"kind":"literal","value":1,"execute":"evil"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<NativeView>(
            json!({"id":"x","layout":"stack","blocks":[],"script":"evil"})
        )
        .is_err()
    );
    let mut m = behavior();
    for key in ["__proto__", "constructor", "prototype", "a.b", "a/b"] {
        let fields = std::collections::HashMap::from([(key, json!({"kind":"literal","value":1}))]);
        set(
            &mut m,
            json!([{"op":"call","action":"save_guides","input":{"kind":"object","fields":fields}}]),
        );
        assert!(m.validate().is_err());
    }
}
#[test]
fn ast_budgets_events_and_references_are_enforced() {
    for steps in [
        json!(vec![json!({"op":"refresh","target":"qty"}); 257]),
        json!([{"op":"refresh","target":"missing"}]),
        json!([{"op":"validate","target":"qty"}]),
        json!([{"op":"navigate","view":"missing"}]),
        json!([{"op":"call","action":"unknown","input":{"kind":"literal","value":{}}}]),
    ] {
        let mut m = behavior();
        set(&mut m, steps);
        assert!(m.validate().is_err());
    }
    let mut step = json!({"op":"msgBox","text":{"en":"Hi"}});
    for _ in 0..10 {
        step = json!({"op":"if","left":{"kind":"literal","value":1},"compare":"eq","right":{"kind":"literal","value":1},"then":[step]});
    }
    let mut m = behavior();
    set(&mut m, json!([step]));
    assert!(m.validate().is_err());
    m = behavior();
    let button = m.views[0]
        .blocks
        .iter_mut()
        .find(|b| b.id == "save_button")
        .unwrap();
    button.kind = "text".into();
    assert!(m.validate().is_err());
}
#[test]
fn context_binding_matches_core_reference_and_every_mount() {
    let mut m = fixture();
    m.entities[0].fields.push(
        serde_json::from_value(
            json!({"name":"product_id","kind":"string","coreReference":"product"}),
        )
        .unwrap(),
    );
    m.views[0].blocks[0].context_binding =
        Some(serde_json::from_value(json!({"field":"product_id","key":"productId"})).unwrap());
    m.surfaces[0].location = "admin.product".into();
    m.validate().unwrap();
    m.surfaces[0].location = "admin.customer".into();
    assert!(m.validate().is_err());
    m.surfaces[0].location = "admin.product".into();
    m.views[0].blocks[0].context_binding.as_mut().unwrap().key = "unknown".into();
    assert!(m.validate().is_err());
}
#[test]
fn view_and_block_limits_reject_oversized_and_duplicate_packages() {
    let mut m = fixture();
    m.views = vec![m.views[0].clone(); 17];
    assert!(m.validate().is_err());
    let mut m = fixture();
    m.views[0].blocks = vec![m.views[0].blocks[0].clone(); 33];
    assert!(m.validate().is_err());
    let mut m = fixture();
    let duplicate = m.views[0].blocks[0].clone();
    m.views[0].blocks.push(duplicate);
    assert!(m.validate().is_err());
}
#[test]
fn record_save_cannot_target_another_entity() {
    let mut m = behavior();
    let mut other = m.entities[0].clone();
    other.name = "other".into();
    m.entities.push(other);
    m.actions
        .iter_mut()
        .find(|a| a.name == "save_guides")
        .unwrap()
        .entity = Some("other".into());
    // Remove the direct form save binding to isolate the code-behind action mismatch.
    m.views[0]
        .blocks
        .iter_mut()
        .find(|b| b.kind == "form")
        .unwrap()
        .write_action = Some("save_form".into());
    let mut save = m
        .actions
        .iter()
        .find(|a| a.name == "save_guides")
        .unwrap()
        .clone();
    save.name = "save_form".into();
    save.entity = Some("guides".into());
    m.actions.push(save);
    m.surfaces[0].actions.push("save_form".into());
    assert_eq!(
        m.validate().unwrap_err().message,
        "Save action targets a different form model"
    );
}
