//! Frozen synthetic snapshots convert into the existing read-only WIT contract; no live data, SQL or credentials are read.
use super::*;
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct Amount {
    pub minor: i64,
    pub currency: String,
    pub scale: u8,
}
impl Amount {
    pub(super) fn valid(&self) -> bool {
        self.minor.unsigned_abs() <= 9_007_199_254_740_991
            && self.currency.len() == 3
            && self.currency.bytes().all(|b| b.is_ascii_uppercase())
            && self.scale <= 3
    }
    fn to_wit(&self) -> wit::Money {
        wit::Money {
            minor: self.minor,
            currency: self.currency.clone(),
            scale: self.scale,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Item {
    pub id: String,
    pub quantity: u32,
    pub price: Amount,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Product {
    pub id: String,
    pub stock: u32,
    pub price: Amount,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub entity: String,
    pub id: String,
    pub revision: u64,
    pub fields: Value,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Frozen {
    pub subtotal: Amount,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub products: Vec<Product>,
    #[serde(default)]
    pub records: Vec<Record>,
}
impl Frozen {
    pub(super) fn validate(&self, m: &Manifest) -> Result<()> {
        let bounded_id = |id: &str| !id.is_empty() && id.len() <= 100;
        let price = |p: &Amount| {
            p.valid()
                && p.currency == self.subtotal.currency
                && p.scale == self.subtotal.scale
                && p.minor >= 0
        };
        if !price(&self.subtotal)
            || self.items.len() > 100
            || self.products.len() > 100
            || self.records.len() > 4
            || self
                .items
                .iter()
                .any(|i| !bounded_id(&i.id) || i.quantity == 0 || !price(&i.price))
            || self
                .products
                .iter()
                .any(|p| !bounded_id(&p.id) || !price(&p.price))
            || self
                .products
                .iter()
                .map(|p| &p.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.products.len()
            || self
                .items
                .iter()
                .map(|p| &p.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.items.len()
            || self
                .records
                .iter()
                .map(|r| (&r.entity, &r.id))
                .collect::<std::collections::HashSet<_>>()
                .len()
                != self.records.len()
        {
            return Err(bad("Invalid frozen commerce snapshot"));
        }
        for record in &self.records {
            if record.revision == 0
                || !record.fields.is_object()
                || !m.commerce_hooks.as_ref().is_some_and(|c| {
                    c.records
                        .iter()
                        .any(|r| r.entity == record.entity && r.id == record.id)
                })
            {
                return Err(bad(
                    "Example records must reference a declared read-only hook record",
                ));
            }
            let entity = m
                .entities
                .iter()
                .find(|e| e.name == record.entity)
                .ok_or(bad("Unknown example entity"))?;
            data::fields(entity, &record.fields)?;
        }
        Ok(())
    }
    pub(super) fn to_wit(&self) -> wit::Snapshot {
        wit::Snapshot {
            cart: wit::CartSnapshot {
                subtotal: self.subtotal.to_wit(),
                items: self
                    .items
                    .iter()
                    .map(|i| wit::CartItem {
                        id: i.id.clone(),
                        quantity: i.quantity,
                        price: i.price.to_wit(),
                    })
                    .collect(),
            },
            products: self
                .products
                .iter()
                .map(|p| {
                    (
                        p.id.clone(),
                        wit::ProductSnapshot {
                            id: p.id.clone(),
                            stock: p.stock,
                            price: p.price.to_wit(),
                        },
                    )
                })
                .collect(),
            records: self
                .records
                .iter()
                .map(|r| {
                    (
                        (r.entity.clone(), r.id.clone()),
                        wit::AppRecord {
                            id: r.id.clone(),
                            revision: r.revision,
                            fields_json: {
                                let mut fields = r.fields.clone();
                                fields["id"] = json!(r.id);
                                fields["revision"] = json!(r.revision);
                                fields.to_string()
                            },
                        },
                    )
                })
                .collect(),
        }
    }
}
