//! Native schema security regressions: bindings, public writes, allowlists, bounded blocks and legacy digests.
use super::*;
fn example() -> Manifest {
    serde_json::from_str(include_str!(
        "../../extensions/apps/care-studio/manifest.json"
    ))
    .unwrap()
}
#[test]
fn care_studio_contract_runs_through_existing_validation() {
    let m = example();
    validate(&m).unwrap();
    assert!(native_views::payload(&m, &m.surfaces[0]).is_some());
}
#[test]
fn native_contract_cannot_smuggle_actions_or_public_forms() {
    for change in 0..7 {
        let mut m = example();
        match change {
            0 => m.views[0].blocks[0].read_action = Some("save_guides".into()),
            1 => m.views[1].blocks[0].kind = "form".into(),
            2 => m.surfaces[0].actions.clear(),
            3 => m.views[0].blocks[0].entity = Some("unknown".into()),
            4 => m.views[0].blocks[0].kind = "javascript".into(),
            5 => m.surfaces[0].ui_path = "native/missing".into(),
            _ => m.views[0].blocks = vec![m.views[0].blocks[0].clone(); 33],
        }
        assert!(validate(&m).is_err(), "mutation {change}");
    }
}
#[test]
fn text_blocks_cannot_bind_an_action_and_read_blocks_cannot_write() {
    let mut m = example();
    m.views[0].blocks[0].kind = "text".into();
    assert!(validate(&m).is_err());
    let mut m = example();
    m.views[0].blocks[0].write_action = Some("save_guides".into());
    assert!(validate(&m).is_err());
}
#[test]
fn public_registry_contains_only_bound_entity_metadata() {
    let mut m = example();
    let mut private = m.entities[0].clone();
    private.name = "private_notes".into();
    private.public_read = false;
    m.entities.push(private);
    let p = native_views::payload(&m, &m.surfaces[1]).unwrap();
    assert_eq!(p["entities"].as_array().unwrap().len(), 1);
}
#[test]
fn empty_native_views_do_not_change_legacy_serialization() {
    let m: Manifest = serde_json::from_str(include_str!(
        "../../extensions/apps/engraving/manifest.json"
    ))
    .unwrap();
    assert!(json!(m).get("views").is_none());
}

#[test]
fn form_geometry_and_presentation_are_validated_on_the_server() {
    let mut m = example();
    m.views[0].layout = "form".into();
    m.views[0].blocks[0].geometry = Some(form_layout::Geometry {
        x: 0,
        y: 0,
        w: 6,
        h: 6,
    });
    validate(&m).unwrap();
    m.views[0].blocks[0].geometry.as_mut().unwrap().x = 11;
    assert!(validate(&m).is_err());
    m.views[0].blocks[0].geometry.as_mut().unwrap().x = 0;
    m.views[0].blocks[0].tab_order = Some(1001);
    assert!(validate(&m).is_err());
}
