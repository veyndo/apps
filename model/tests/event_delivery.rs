//! Independent delivery declarations and privacy-safe filters retain bounded grammar and required host destination rejection.
use serde_json::{Value, json};
use std::{cell::Cell, collections::HashMap};
use veyndo_app_model::{
    events::{self, DestinationPolicy, EventDelivery, EventFilter},
    package::CompileError,
    subscriptions,
};
#[path = "support/events.rs"]
mod support;
use support::Fixture;
struct Host {
    allow: bool,
    calls: Cell<usize>,
}
impl DestinationPolicy for Host {
    type Error = &'static str;
    fn validate_destination(&self, _: &str) -> Result<(), Self::Error> {
        self.calls.set(self.calls.get() + 1);
        if self.allow {
            Ok(())
        } else {
            Err("host destination denied")
        }
    }
}
fn filter(event: &str, equals: Value) -> EventFilter {
    serde_json::from_value(json!({"event":event,"equals":equals})).unwrap()
}
#[test]
fn filters_only_observe_projected_fields_and_or_groups_keep_scalar_equality() {
    let mut f = Fixture::new(&["order.*"], &["events:order.*"]);
    f.filters = vec![filter("order.*", json!({"email":"private"}))];
    let source = json!({"orderId":"o","currency":"EUR","email":"private","count":0});
    assert!(!events::filtered(&f.context(), "order.placed", &source));
    assert!(!subscriptions::eligible(
        &f.context(),
        "order.placed",
        &source
    ));
    f.filters = vec![
        filter("order.*", json!({"currency":"USD"})),
        filter("order.*", json!({"currency":"EUR","orderId":"o"})),
    ];
    assert!(events::filtered(&f.context(), "order.placed", &source));
    f.filters = vec![filter(
        "order.*",
        json!({"currency":"EUR","orderId":"other"}),
    )];
    assert!(!events::filtered(&f.context(), "order.placed", &source));
    assert!(events::filtered(
        &f.context(),
        "consumer.requested",
        &source
    ));
    assert!(!events::matches("order.*", "orders.placed"));
    assert!(!events::matches("order.*", "order"));
    assert!(events::matches("order.*", "order.payment.updated"));
}
#[test]
fn event_filter_grammar_and_budgets_fail_closed() {
    let host = Host {
        allow: false,
        calls: Cell::new(0),
    };
    for bad in [
        filter("missing", json!({"currency":"EUR"})),
        filter("order.placed", json!({})),
        filter("order.placed", json!({"a":1,"b":2,"c":3,"d":4,"e":5})),
        filter("order.placed", json!({"currency":[]})),
        filter("order.placed", json!({"currency":{}})),
        filter("order.placed", json!({"Bad key":1})),
        filter("order.placed", json!({"currency":"x".repeat(1000)})),
    ] {
        let mut f = Fixture::new(&["order.placed"], &["events:order.placed"]);
        f.filters = vec![bad];
        assert!(events::validate(&f.context(), None, &host).is_err());
    }
    let mut f = Fixture::new(&["order.placed"], &["events:order.placed"]);
    f.filters = vec![filter("order.placed", json!({"currency":"EUR"})); 24];
    assert!(events::validate(&f.context(), None, &host).is_ok());
    f.filters.push(f.filters[0].clone());
    assert!(events::validate(&f.context(), None, &host).is_err());
    assert_eq!(host.calls.get(), 0);
}
#[test]
fn destination_policy_is_required_and_host_rejection_cannot_be_overridden() {
    let f = Fixture::new(&["order.placed"], &["events:order.placed", "events.send"]);
    let remote = EventDelivery {
        url: Some("https://example.test/events".into()),
        batch_size: 25,
    };
    let denied = Host {
        allow: false,
        calls: Cell::new(0),
    };
    assert!(matches!(
        events::validate(&f.context(), Some(&remote), &denied),
        Err(CompileError::Host("host destination denied"))
    ));
    assert_eq!(denied.calls.get(), 1);
    let accepted = Host {
        allow: true,
        calls: Cell::new(0),
    };
    events::validate(&f.context(), Some(&remote), &accepted).unwrap();
    assert_eq!(accepted.calls.get(), 1);
    let no_send = Fixture::new(&["order.placed"], &["events:order.placed"]);
    assert!(matches!(
        events::validate(&no_send.context(), Some(&remote), &accepted),
        Err(CompileError::Model(_))
    ));
    let local = EventDelivery {
        url: None,
        batch_size: 1,
    };
    events::validate(&f.context(), Some(&local), &denied).unwrap();
    assert_eq!(denied.calls.get(), 1);
    for size in [0, 26, u16::MAX] {
        assert!(
            events::validate(
                &f.context(),
                Some(&EventDelivery {
                    url: None,
                    batch_size: size
                }),
                &denied
            )
            .is_err()
        );
    }
    let empty = Fixture::new(&[], &[]);
    assert!(events::validate(&empty.context(), Some(&local), &denied).is_err());
}
#[test]
fn strict_event_wire_preserves_legacy_delivery_defaults_and_unknown_field_rejection() {
    let delivery: EventDelivery = serde_json::from_value(json!({})).unwrap();
    assert_eq!(delivery.batch_size, 1);
    assert_eq!(
        serde_json::to_value(delivery).unwrap(),
        json!({"batchSize":1})
    );
    assert!(
        serde_json::from_value::<EventDelivery>(json!({"url":null,"batchSize":1,"secret":"x"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<EventFilter>(
            json!({"event":"order.placed","equals":{},"sql":"x"})
        )
        .is_err()
    );
    let filter = EventFilter {
        event: "order.placed".into(),
        equals: HashMap::from([("currency".into(), json!("EUR"))]),
    };
    assert_eq!(
        serde_json::to_value(filter).unwrap(),
        json!({"event":"order.placed","equals":{"currency":"EUR"}})
    );
}
