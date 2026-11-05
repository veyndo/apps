//! Clock-injected cron and private webhook declarations remain bounded and share strict action input validation.
use serde_json::json;
use veyndo_app_model::{
    Action,
    triggers::{self, AppSchedule, AppWebhook},
};
fn clock() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-10-05T10:01:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc)
}
fn action() -> Action {
    serde_json::from_value(json!({"name":"tick","description":"Tick","handler":"emit","public":false,"mcp":false,"inputSchema":{"type":"object","properties":{"limit":{"type":"integer"}},"required":["limit"],"additionalProperties":false}})).unwrap()
}
fn schedule() -> AppSchedule {
    serde_json::from_value(json!({"id":"daily","cron":"0 */15 * * * *","action":"tick","input":{"limit":2},"enabled":true})).unwrap()
}
#[test]
fn cron_uses_explicit_utc_clock_with_one_second_value_and_has_repeatable_next_tick() {
    for expression in [
        "* * * * * *",
        "0 * * * *",
        "61 * * * * *",
        "0 99 * * * *",
        "0,30 * * * * *",
        "*/10 * * * * *",
    ] {
        assert!(triggers::parse(expression).is_err(), "{expression}");
    }
    let parsed = triggers::parse("0 */15 * * * *").unwrap();
    assert_eq!(
        parsed.after(&clock()).next().unwrap().to_rfc3339(),
        "2026-10-05T10:15:00+00:00"
    );
    let too_long = format!("0{}* * * * *", " ".repeat(101));
    assert!(triggers::parse(&too_long).is_err());
}
#[test]
fn schedule_inputs_and_action_bindings_use_the_same_closed_model_validator() {
    let s = schedule();
    let a = action();
    triggers::schedules(std::slice::from_ref(&s), std::slice::from_ref(&a), clock()).unwrap();
    for input in [
        json!({}),
        json!({"limit":"2"}),
        json!({"limit":2,"undeclared":true}),
        json!("bad"),
        json!({"limit":"x".repeat(16384)}),
    ] {
        let mut bad = s.clone();
        bad.input = input;
        assert!(triggers::schedules(&[bad], std::slice::from_ref(&a), clock()).is_err());
    }
    let mut bad_action = a.clone();
    bad_action.handler = "service".into();
    assert!(triggers::schedules(std::slice::from_ref(&s), &[bad_action], clock()).is_err());
    let mut bad = s.clone();
    bad.action = "missing".into();
    assert!(triggers::schedules(&[bad], &[a], clock()).is_err());
}
#[test]
fn duplicate_ids_invalid_cron_and_excess_schedules_fail_even_when_disabled() {
    let a = action();
    let mut s = schedule();
    s.enabled = false;
    triggers::schedules(std::slice::from_ref(&s), std::slice::from_ref(&a), clock()).unwrap();
    assert!(
        triggers::schedules(&[s.clone(), s.clone()], std::slice::from_ref(&a), clock()).is_err()
    );
    let mut many = Vec::new();
    for i in 0..8 {
        let mut v = s.clone();
        v.id = format!("tick{i}");
        many.push(v);
    }
    triggers::schedules(&many, std::slice::from_ref(&a), clock()).unwrap();
    let mut extra = s.clone();
    extra.id = "extra".into();
    many.push(extra);
    assert!(triggers::schedules(&many, std::slice::from_ref(&a), clock()).is_err());
    s.cron = "* * * * * *".into();
    assert!(triggers::schedules(&[s], std::slice::from_ref(&a), clock()).is_err());
    let mut s = schedule();
    s.id = "Bad id".into();
    assert!(triggers::schedules(&[s], &[a], clock()).is_err());
}
#[test]
fn incoming_webhooks_require_unique_bounded_ids_and_declared_private_emit_actions() {
    let hook = AppWebhook {
        id: "incoming".into(),
        action: "tick".into(),
    };
    let a = action();
    triggers::webhooks(std::slice::from_ref(&hook), std::slice::from_ref(&a)).unwrap();
    for (public, handler) in [(true, "emit"), (false, "service"), (true, "service")] {
        let mut bad = a.clone();
        bad.public = public;
        bad.handler = handler.into();
        assert!(triggers::webhooks(std::slice::from_ref(&hook), &[bad]).is_err());
    }
    assert!(triggers::webhooks(std::slice::from_ref(&hook), &[]).is_err());
    assert!(triggers::webhooks(&[hook.clone(), hook.clone()], std::slice::from_ref(&a)).is_err());
    let mut many: Vec<_> = (0..8)
        .map(|i| AppWebhook {
            id: format!("hook{i}"),
            action: "tick".into(),
        })
        .collect();
    triggers::webhooks(&many, std::slice::from_ref(&a)).unwrap();
    many.push(hook);
    assert!(triggers::webhooks(&many, std::slice::from_ref(&a)).is_err());
    assert!(
        triggers::webhooks(
            &[AppWebhook {
                id: "Bad id".into(),
                action: "tick".into()
            }],
            &[a]
        )
        .is_err()
    );
}
#[test]
fn trigger_wire_preserves_defaults_and_rejects_inline_keys_raw_sql_and_unknown_fields() {
    let raw = json!({"id":"daily","cron":"0 */15 * * * *","action":"tick","input":{"limit":2},"enabled":true});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<AppSchedule>(raw.clone()).unwrap()).unwrap(),
        raw
    );
    let mut bad = raw.clone();
    bad["sql"] = json!("x");
    assert!(serde_json::from_value::<AppSchedule>(bad).is_err());
    let mut missing = raw;
    missing.as_object_mut().unwrap().remove("enabled");
    assert!(serde_json::from_value::<AppSchedule>(missing).is_err());
    assert!(
        serde_json::from_value::<AppWebhook>(json!({"id":"incoming","action":"tick","secret":"x"}))
            .is_err()
    );
    let raw = json!({"id":"incoming","action":"tick"});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<AppWebhook>(raw.clone()).unwrap()).unwrap(),
        raw
    );
}
