//! Run a real read-only commerce hook independently; the caller supplies its admitted snapshot and resource policy.
use std::collections::HashMap;
use veyndo_app_runtime::{
    component_runtime::{CartSnapshot, Kind, Module, Money, Snapshot},
    policy::{ResourcePolicy, Resources},
};
struct Host;
impl ResourcePolicy for Host {
    fn admitted(&self, r: Resources) -> bool {
        r.tables <= 1 && r.memories <= 1 && r.elements <= 10_000 && r.pages <= 16
    }
}
fn main() -> Result<(), String> {
    let minor = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "10000".into())
        .parse::<i64>()
        .map_err(|_| "Pass an integer amount in minor units")?;
    let module = Module::new(
        include_str!("../tests/fixtures/snapshot-price.component.wat"),
        &Host,
        "",
    )?;
    module.validate_interface()?;
    let snapshot = Snapshot {
        cart: CartSnapshot {
            items: vec![],
            subtotal: Money {
                minor,
                currency: "EUR".into(),
                scale: 2,
            },
        },
        products: HashMap::new(),
        records: HashMap::new(),
    };
    let outcome = module.evaluate(snapshot, Kind::Price, "{}")?;
    println!(
        "Read-only guest adjustment: {} {} minor units; accepted: {}; reason: {}",
        outcome.adjustment.minor,
        outcome.adjustment.currency,
        outcome.accepted,
        outcome.reason_code
    );
    Ok(())
}
