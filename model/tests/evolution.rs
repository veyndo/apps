//! Independent upgrade scenarios retain explicit transformations and reject destructive or relational shortcuts.
use serde_json::{Value, json};
use veyndo_app_model::{
    Entity, Field, Kind,
    evolution::{self, Migration},
};
fn entity(fields: Value) -> Entity {
    serde_json::from_value(json!({"name":"items","fields":fields})).unwrap()
}
fn path(steps: Value) -> Migration {
    serde_json::from_value(json!({"fromVersion":"1.0.0","steps":steps})).unwrap()
}
#[test]
fn explicit_rename_convert_fill_remove_plan_preserves_old_model() {
    let old = entity(
        json!([{"name":"title","kind":"string"},{"name":"quantity","kind":"string"},{"name":"obsolete","kind":"string"}]),
    );
    let next = entity(
        json!([{"name":"name","kind":"string"},{"name":"quantity","kind":"integer"},{"name":"enabled","kind":"boolean","required":true}]),
    );
    let plan = path(json!([
        {"op":"rename","entity":"items","field":"title","to":"name"},
        {"op":"convert","entity":"items","field":"quantity"},
        {"op":"fill","entity":"items","field":"enabled","value":true},
        {"op":"remove","entity":"items","field":"obsolete"}
    ]));
    evolution::validate("2.0.0", std::slice::from_ref(&plan)).unwrap();
    let transformed = evolution::shape(
        std::slice::from_ref(&old),
        std::slice::from_ref(&next),
        Some(&plan),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&transformed[0]).unwrap(),
        serde_json::to_value(&next).unwrap()
    );
    assert_eq!(old.fields[0].name, "title");
    assert_eq!(
        evolution::shape(&[old], &[next], None).err().unwrap().kind,
        Kind::Conflict
    );
}
#[test]
fn strict_declarations_preserve_wire_and_block_raw_sql_unknowns_and_reserved_keys() {
    let raw = json!({"fromVersion":"1.0.0","steps":[{"op":"fill","entity":"items","field":"count","value":0}]});
    let parsed: Migration = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), raw);
    for steps in [
        json!([{"op":"sql","entity":"items","field":"count","sql":"DROP TABLE products"}]),
        json!([{"op":"remove","entity":"items","field":"count","sql":"x"}]),
    ] {
        assert!(
            serde_json::from_value::<Migration>(json!({"fromVersion":"1","steps":steps})).is_err()
        );
    }
    for key in ["id", "tenant", "revision", "Bad key"] {
        for step in [
            json!({"op":"remove","entity":"items","field":key}),
            json!({"op":"rename","entity":"items","field":"title","to":key}),
        ] {
            assert!(evolution::validate("2", &[path(json!([step]))]).is_err());
        }
    }
}
#[test]
fn migration_paths_and_defaults_are_bounded_and_unambiguous() {
    let one = path(json!([{"op":"remove","entity":"items","field":"old"}]));
    assert!(evolution::validate("1.0.0", std::slice::from_ref(&one)).is_err());
    assert!(evolution::validate("2", &[one.clone(), one.clone()]).is_err());
    let mut many: Vec<_> = (0..16)
        .map(|i| {
            let mut p = one.clone();
            p.from_version = i.to_string();
            p
        })
        .collect();
    evolution::validate("2.0.0", &many).unwrap();
    let mut extra = one.clone();
    extra.from_version = "extra".into();
    many.push(extra);
    assert!(evolution::validate("2.0.0", &many).is_err());
    assert!(evolution::validate("2", &[path(json!([]))]).is_err());
    let mut p = one.clone();
    p.steps = vec![p.steps[0].clone(); 33];
    assert!(evolution::validate("2", &[p]).is_err());
    let oversized =
        path(json!([{"op":"fill","entity":"items","field":"title","value":"x".repeat(32768)}]));
    assert!(evolution::validate("2", &[oversized]).is_err());
}
#[test]
fn operational_and_core_binding_changes_require_offline_migration() {
    let mut old = entity(json!([{"name":"title","kind":"string"}]));
    old.operational = Some(true);
    let mut next = old.clone();
    next.operational = None;
    assert_eq!(
        evolution::compatible(&old, &next).unwrap_err().kind,
        Kind::Conflict
    );
    next = old.clone();
    next.extends = Some(
        serde_json::from_value(json!({"version":1,"object":"product","field":"product_id"}))
            .unwrap(),
    );
    assert_eq!(
        evolution::compatible(&old, &next).unwrap_err().kind,
        Kind::Conflict
    );
    assert!(evolution::shape(std::slice::from_ref(&old), &[], None).is_err());
}
#[test]
fn relationship_migrations_and_rename_collisions_do_not_pass() {
    for field in [
        json!({"name":"link","kind":"string","references":"items"}),
        json!({"name":"link","kind":"string","coreReference":"product"}),
        json!({"name":"link","kind":"image"}),
        json!({"name":"link","kind":"file"}),
        json!({"name":"link","kind":"relations","references":"items"}),
    ] {
        let old = entity(json!([field]));
        let p = path(json!([{"op":"remove","entity":"items","field":"link"}]));
        assert!(
            evolution::shape(
                std::slice::from_ref(&old),
                std::slice::from_ref(&old),
                Some(&p)
            )
            .is_err()
        );
    }
    let old = entity(json!([{"name":"a","kind":"string"},{"name":"b","kind":"string"}]));
    for step in [
        json!({"op":"rename","entity":"items","field":"a","to":"b"}),
        json!({"op":"remove","entity":"items","field":"missing"}),
        json!({"op":"convert","entity":"items","field":"missing"}),
        json!({"op":"remove","entity":"missing","field":"a"}),
    ] {
        assert!(
            evolution::shape(
                std::slice::from_ref(&old),
                std::slice::from_ref(&old),
                Some(&path(json!([step])))
            )
            .is_err()
        );
    }
    let next = entity(json!([{"name":"a","kind":"integer"},{"name":"b","kind":"string"}]));
    assert!(
        evolution::shape(
            &[old],
            &[next],
            Some(&path(
                json!([{"op":"fill","entity":"items","field":"a","value":0}])
            ))
        )
        .is_err()
    );
}
#[test]
fn exact_conversions_reject_fractional_and_overflow_integer_without_float_money() {
    let f: Field = serde_json::from_value(json!({"name":"quantity","kind":"integer"})).unwrap();
    assert_eq!(
        evolution::convert(&json!("9223372036854775807"), &f).unwrap(),
        json!(i64::MAX)
    );
    for bad in ["2.5", "9223372036854775808", "NaN"] {
        assert!(evolution::convert(&json!(bad), &f).is_err());
    }
    let decimal: Field = serde_json::from_value(json!({"name":"price","kind":"decimal"})).unwrap();
    assert_eq!(
        evolution::convert(&json!(9007199254740993_i64), &decimal).unwrap(),
        json!("9007199254740993")
    );
    assert!(
        evolution::convert(&Value::Null, &decimal)
            .unwrap()
            .is_null()
    );
    let boolean: Field =
        serde_json::from_value(json!({"name":"enabled","kind":"boolean"})).unwrap();
    assert_eq!(
        evolution::convert(&json!("false"), &boolean).unwrap(),
        json!(false)
    );
    assert_eq!(
        evolution::convert(&json!("TRUE"), &boolean).unwrap(),
        json!("TRUE")
    );
}
#[test]
fn adding_choices_is_compatible_but_removal_and_unannounced_field_changes_are_not() {
    let old = entity(
        json!([{"name":"size","kind":"string","choices":[{"value":"small","label":{"de-DE":"Klein"}}]}]),
    );
    let mut next = old.clone();
    next.fields[0]
        .choices
        .push(serde_json::from_value(json!({"value":"large","label":{"de-DE":"Groß"}})).unwrap());
    evolution::shape(std::slice::from_ref(&old), &[next], None).unwrap();
    let mut next = old.clone();
    next.fields[0].choices.clear();
    assert!(evolution::shape(std::slice::from_ref(&old), &[next], None).is_err());
    let mut next = old.clone();
    next.fields[0].required = true;
    assert!(evolution::shape(&[old], &[next], None).is_err());
}
