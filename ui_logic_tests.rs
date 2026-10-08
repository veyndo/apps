//! UI programs must keep static types, same-model saves, surface allowlists and acyclic ownership before install or preview.
use super::*;
fn example() -> Manifest {
    let mut m: Manifest = serde_json::from_str(include_str!(
        "../../extensions/apps/care-studio/manifest.json"
    ))
    .unwrap();
    let form = m.views[0]
        .blocks
        .iter()
        .find(|b| b.kind == "form")
        .unwrap()
        .id
        .clone();
    let mut button: native_views::NativeBlock=serde_json::from_value(json!({"id":"save_button","kind":"button","title":{"en":"Save"},"handlers":{"click":[{"op":"validate","target":form},{"op":"call","action":"save_guides","input":{"kind":"record","block":form}},{"op":"refresh","target":m.views[0].blocks[0].id}]}})).unwrap();
    m.views[0].blocks.push(button.clone());
    button.id = "container".into();
    button.kind = "frame".into();
    button.handlers = None;
    button.child_blocks = vec!["save_button".into()];
    m.views[0].blocks.push(button);
    m
}
#[test]
fn code_behind_runs_only_approved_actions_and_valid_references() {
    let m = example();
    validate(&m).unwrap();
    for change in 0..5 {
        let mut v = json!(m.clone());
        let steps = &mut v["views"][0]["blocks"][2]["handlers"]["click"];
        match change {
            0 => steps[1]["action"] = json!("unknown"),
            1 => steps[1]["input"]["block"] = json!("save_button"),
            2 => v["surfaces"][0]["actions"] = json!(["list_guides"]),
            3 => v["views"][0]["blocks"][3]["childBlocks"] = json!(["container"]),
            _ => v["views"][0]["blocks"][3]["childBlocks"] = json!(["save_button", "save_button"]),
        }
        assert!(
            validate(&serde_json::from_value(v).unwrap()).is_err(),
            "change {change}"
        );
    }
}
#[test]
fn input_literals_do_not_coerce_and_ordered_comparisons_are_typed() {
    let mut m = example();
    m.entities[0]
        .fields
        .push(serde_json::from_value(json!({"name":"quantity","kind":"integer"})).unwrap());
    m.views[0].blocks.push(serde_json::from_value(json!({"id":"qty","kind":"textbox","title":{"en":"Quantity"},"entity":"guides","dataField":"quantity","readAction":"list_guides"})).unwrap());
    let button = m.views[0]
        .blocks
        .iter()
        .position(|b| b.id == "save_button")
        .unwrap();
    m.views[0].blocks[button].handlers=Some(serde_json::from_value(json!({"click":[{"op":"set","target":"qty","value":{"kind":"literal","value":10}},{"op":"if","left":{"kind":"value","block":"qty"},"compare":"gt","right":{"kind":"literal","value":5},"then":[],"otherwise":[]}]})).unwrap());
    validate(&m).unwrap();
    let mut invalid = json!(m.clone());
    invalid["views"][0]["blocks"][button]["handlers"]["click"][0]["value"]["value"] = json!("10");
    assert!(validate(&serde_json::from_value(invalid).unwrap()).is_err());
    let mut invalid = json!(m);
    invalid["views"][0]["blocks"][button]["handlers"]["click"][1]["right"]["value"] = json!("5");
    assert!(validate(&serde_json::from_value(invalid).unwrap()).is_err());
}
