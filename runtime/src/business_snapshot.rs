//! Deterministic projection of admitted synthetic examples into the original read-only WIT ABI.
use crate::component_runtime as wit;
use serde_json::json;
use veyndo_app_model::business_snapshot::{Amount, Frozen, Record};
fn amount(value: &Amount) -> wit::Money {
    wit::Money {
        minor: value.minor,
        currency: value.currency.clone(),
        scale: value.scale,
    }
}
fn record(value: &Record) -> ((String, String), wit::AppRecord) {
    let mut fields = value.fields.clone();
    fields["id"] = json!(value.id);
    fields["revision"] = json!(value.revision);
    (
        (value.entity.clone(), value.id.clone()),
        wit::AppRecord {
            id: value.id.clone(),
            revision: value.revision,
            fields_json: fields.to_string(),
        },
    )
}
pub fn snapshot(value: &Frozen) -> wit::Snapshot {
    wit::Snapshot {
        cart: wit::CartSnapshot {
            subtotal: amount(&value.subtotal),
            items: value
                .items
                .iter()
                .map(|i| wit::CartItem {
                    id: i.id.clone(),
                    quantity: i.quantity,
                    price: amount(&i.price),
                })
                .collect(),
        },
        products: value
            .products
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    wit::ProductSnapshot {
                        id: p.id.clone(),
                        stock: p.stock,
                        price: amount(&p.price),
                    },
                )
            })
            .collect(),
        records: value.records.iter().map(record).collect(),
    }
}
