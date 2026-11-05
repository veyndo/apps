//! Independent event subscription and minimized payload scenarios, including own-app, B2B and dual PII/financial grants.
use serde_json::json;
use veyndo_app_model::subscriptions;
#[path = "support/events.rs"]
mod support;
use support::Fixture;
#[test]
fn subscriptions_need_both_declaration_and_matching_package_permission() {
    for (declared, rights, kind, allow) in [
        (vec!["product.updated"], vec![], "product.updated", false),
        (vec![], vec!["events:product.*"], "product.updated", false),
        (
            vec!["product.*"],
            vec!["events:product.*"],
            "product.updated",
            true,
        ),
        (
            vec!["product.updated"],
            vec!["events:product.updated"],
            "product.updated",
            true,
        ),
        (
            vec!["product.*"],
            vec!["events:product.*"],
            "products.updated",
            false,
        ),
        (
            vec!["customer.created"],
            vec!["events.read"],
            "customer.created",
            false,
        ),
        (
            vec!["team.updated"],
            vec!["events.read"],
            "team.updated",
            false,
        ),
        (
            vec!["order.placed"],
            vec!["events.read"],
            "order.placed",
            true,
        ),
        (
            vec!["consumer.requested"],
            vec!["events.read"],
            "consumer.requested",
            true,
        ),
    ] {
        let f = Fixture::new(&declared, &rights);
        assert_eq!(
            subscriptions::subscribed(&f.context(), kind),
            allow,
            "{kind}"
        );
    }
    let denied = Fixture::new(&["product.*"], &["events:order.*"]);
    assert!(subscriptions::validate(&denied.context()).is_err());
    let accepted = Fixture::new(&["product.*"], &["events:product.*"]);
    subscriptions::validate(&accepted.context()).unwrap();
}
#[test]
fn app_events_are_namespaced_and_generic_app_records_are_owner_bound() {
    let own = Fixture::new(&["app.advice.*"], &["events:self"]);
    assert!(subscriptions::eligible(
        &own.context(),
        "app.advice.completed",
        &json!({"custom":"own"})
    ));
    assert!(!subscriptions::subscribed(
        &own.context(),
        "app.other.completed"
    ));
    let record = Fixture::new(
        &["app.record.changed", "app.configuration.changed"],
        &["events:app.*"],
    );
    for kind in ["app.record.changed", "app.configuration.changed"] {
        assert!(subscriptions::eligible(
            &record.context(),
            kind,
            &json!({"app":"advice","id":"one"})
        ));
        assert!(!subscriptions::eligible(
            &record.context(),
            kind,
            &json!({"app":"other","id":"one"})
        ));
        assert!(!subscriptions::eligible(
            &record.context(),
            kind,
            &json!({"id":"one"})
        ));
    }
    assert_eq!(
        subscriptions::payload(&own.context(), "app.advice.completed", json!({"custom":1})),
        json!({"custom":1})
    );
}
#[test]
fn pii_and_financial_instructions_keep_the_original_separate_permissions() {
    let source = json!({"orderId":"o","attemptId":"a","paymentInstructions":{"iban":"synthetic"},"email":"private"});
    for scopes in [
        vec![],
        vec!["customers.pii"],
        vec!["payments.instructions.read"],
    ] {
        let f = Fixture::new(&[], &scopes);
        assert_eq!(
            subscriptions::payload(
                &f.context(),
                "payment.instructions.available",
                source.clone()
            ),
            json!({"orderId":"o","attemptId":"a"})
        );
    }
    let both = Fixture::new(&[], &["customers.pii", "payments.instructions.read"]);
    assert_eq!(
        subscriptions::payload(
            &both.context(),
            "payment.instructions.available",
            source.clone()
        ),
        source
    );
    let pii = Fixture::new(&[], &["customers.pii"]);
    let source = json!({"orderId":"o","email":"private"});
    assert_eq!(
        subscriptions::payload(&pii.context(), "order.placed", source.clone()),
        source
    );
}
#[test]
fn commerce_pos_fulfillment_and_intelligence_payloads_are_minimized() {
    let f = Fixture::new(&[], &[]);
    let source = json!({"orderId":"o","id":"i","revision":3,"currency":"EUR","totalMinor":123,"stock":4,"documentId":"d","deliveryId":"delivery","claimId":"c","state":"ready","email":"private","address":{"street":"private"},"token":"private","evidence":"private","cashier":"private"});
    for (kind, want) in [
        (
            "order.placed",
            json!({"orderId":"o","currency":"EUR","totalMinor":123,"state":"ready"}),
        ),
        (
            "pos.sale.completed",
            json!({"orderId":"o","currency":"EUR","totalMinor":123}),
        ),
        (
            "delivery.dispatched",
            json!({"orderId":"o","deliveryId":"delivery","revision":3,"state":"ready"}),
        ),
        ("document.created", json!({"orderId":"o","documentId":"d"})),
        ("product.updated", json!({"id":"i","revision":3,"stock":4})),
        (
            "intelligence.claim.updated",
            json!({"id":"i","claimId":"c","revision":3,"state":"ready"}),
        ),
        ("unknown.event", json!({"id":"i","revision":3})),
    ] {
        assert_eq!(
            subscriptions::payload(&f.context(), kind, source.clone()),
            want,
            "{kind}"
        );
    }
    assert_eq!(
        subscriptions::payload(&f.context(), "order.placed", json!(["bad"])),
        json!({})
    );
}

#[test]
fn b2b_requires_explicit_domain_read_even_with_event_scope_and_pii() {
    let source = json!({"companyId":"c","unitId":"u","totalMinor":200,"buyerEmail":"private"});
    let no_read = Fixture::new(&["b2b.*"], &["events:b2b.*", "customers.pii"]);
    assert!(!subscriptions::subscribed(
        &no_read.context(),
        "b2b.company.updated"
    ));
    assert_eq!(
        subscriptions::payload(&no_read.context(), "b2b.company.updated", source.clone()),
        json!({})
    );
    let read = Fixture::new(&["b2b.*"], &["events:b2b.*", "b2b.read", "customers.pii"]);
    assert!(subscriptions::subscribed(
        &read.context(),
        "b2b.company.updated"
    ));
    assert_eq!(
        subscriptions::payload(&read.context(), "b2b.company.updated", source),
        json!({"companyId":"c","unitId":"u","totalMinor":200})
    );
}
