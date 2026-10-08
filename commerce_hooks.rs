//! Typed pure commerce hooks share the compiled sandbox cache and receive explicit immutable snapshots, never SQL or credentials.
use super::*;
use std::sync::Arc;
use vendune::component_runtime::{
    self as wit, AppRecord, CartItem, CartSnapshot, Money, ProductSnapshot, Snapshot,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Contract {
    pub source: String,
    pub hooks: Vec<String>,
    #[serde(default)]
    pub records: Vec<Record>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub entity: String,
    pub id: String,
}
pub(crate) struct Prepared {
    pub modules: HashMap<String, Arc<Sandbox>>,
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
    if let Some(c) = &m.commerce_hooks
        && (!m.permissions.iter().any(|p| p == "commerce.hooks")
            || c.source.len() > 32768
            || !Sandbox::component_source(&c.source)
            || c.hooks.is_empty()
            || c.hooks.len() > 4
            || c.hooks
                .iter()
                .any(|h| !["price", "discount", "shipping", "validation"].contains(&h.as_str()))
            || c.hooks
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != c.hooks.len()
            || c.records.len() > 4
            || c.records.iter().any(|r| {
                r.id.is_empty()
                    || r.id.len() > 100
                    || !m.entities.iter().any(|e| e.name == r.entity)
            })
            || (!c.records.is_empty() && !m.permissions.iter().any(|p| p == "data.read")))
    {
        return Err(bad("Invalid typed commerce hook contract"));
    }
    Ok(())
}
async fn rows(conn: &mut sqlx::PgConnection, t: &str) -> Result<Vec<sqlx::postgres::PgRow>> {
    // Row locks fence upgrades/deactivation during checkout. Nine detects the eight-app admission cap.
    let rows=sqlx::query("SELECT manifest FROM app_packages WHERE tenant=$1 AND active AND manifest ? 'commerceHooks' ORDER BY id LIMIT 9 FOR SHARE").bind(t).fetch_all(conn).await?;
    if rows.len() > 8 {
        return Err(bad("At most eight active commerce-hook apps are admitted"));
    }
    Ok(rows)
}
pub(super) async fn admission(conn: &mut sqlx::PgConnection, t: &str, m: &Manifest) -> Result<()> {
    if m.commerce_hooks.is_some() {
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM app_packages WHERE tenant=$1 AND active AND id<>$2 AND manifest ? 'commerceHooks'").bind(t).bind(&m.id).fetch_one(conn).await?;
        if count >= 8 {
            return Err(bad("At most eight active commerce-hook apps are admitted"));
        }
    }
    Ok(())
}
pub(crate) async fn prepare(a: &App, t: &str) -> Result<Prepared> {
    let mut conn = a.db.begin().await?;
    let ms = rows(&mut conn, t).await?;
    conn.rollback().await?;
    let mut modules = HashMap::new();
    for row in ms {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid hook package"))?;
        let c = m.commerce_hooks.unwrap();
        modules.insert(hash(&c.source), runtime::prepare_source(c.source).await?);
    }
    Ok(Prepared { modules })
}
fn money(amount: f64, code: &str, scale: u8) -> Result<Money> {
    Ok(Money {
        minor: currencies::amount(amount, code, scale)?.minor(),
        currency: code.into(),
        scale,
    })
}
fn snapshot(
    c: &StoredCart,
    ps: &[Product],
    q: &Value,
    records: HashMap<(String, String), AppRecord>,
) -> Result<Snapshot> {
    let code = q["price"]["currency"]
        .as_str()
        .ok_or(bad("Quote currency missing"))?;
    let scale = q["price"]["currencyScale"]
        .as_u64()
        .ok_or(bad("Quote scale missing"))? as u8;
    let mut products = HashMap::new();
    let mut items = vec![];
    for line in q["lineItems"]
        .as_array()
        .ok_or(bad("Quote items missing"))?
    {
        let id = line["id"].as_str().ok_or(bad("Quote item ID missing"))?;
        let p = ps
            .iter()
            .find(|p| p.id == id)
            .ok_or(bad("Product snapshot missing"))?;
        let price = money(
            line["price"]["unitPrice"]
                .as_f64()
                .ok_or(bad("Quote unit price missing"))?,
            code,
            scale,
        )?;
        products.insert(
            id.into(),
            ProductSnapshot {
                id: id.into(),
                stock: u32::try_from(p.stock).map_err(|_| bad("Invalid stock"))?,
                price: price.clone(),
            },
        );
        items.push(CartItem {
            id: id.into(),
            quantity: c
                .data
                .items
                .iter()
                .find(|i| i.id == id)
                .ok_or(bad("Cart item missing"))?
                .quantity,
            price,
        });
    }
    Ok(Snapshot {
        cart: CartSnapshot {
            items,
            subtotal: money(
                q["price"]["positionPrice"]
                    .as_f64()
                    .ok_or(bad("Quote subtotal missing"))?,
                code,
                scale,
            )?,
        },
        products,
        records,
    })
}
pub(crate) async fn apply(
    conn: &mut sqlx::PgConnection,
    c: &StoredCart,
    ps: &[Product],
    mut q: Value,
    s: &commerce::Settings,
    revision: i64,
    prepared: &Prepared,
) -> Result<Value> {
    let ms = rows(conn, &c.tenant).await?;
    let mut audit = vec![];
    for row in ms {
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid hook package"))?;
        let contract = m
            .commerce_hooks
            .as_ref()
            .ok_or(bad("Hook contract missing"))?;
        let guest = prepared
            .modules
            .get(&hash(&contract.source))
            .cloned()
            .ok_or(conflict("Commerce hooks changed; review checkout again"))?;
        let mut records = HashMap::new();
        for r in &contract.records {
            let name = table(&c.tenant, &m.id, &r.entity);
            let sql = format!(
                "SELECT to_jsonb(x) AS data FROM public.{name} x WHERE tenant=$1 AND id=$2 FOR SHARE"
            );
            let value: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()))
                .bind(&c.tenant)
                .bind(&r.id)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or(conflict("Hook record missing"))?;
            let revision = value["revision"]
                .as_u64()
                .ok_or(bad("Hook record revision missing"))?;
            let mut fields = value;
            fields.as_object_mut().unwrap().remove("tenant");
            records.insert(
                (r.entity.clone(), r.id.clone()),
                AppRecord {
                    id: r.id.clone(),
                    revision,
                    fields_json: fields.to_string(),
                },
            );
        }
        let mut record_revisions: Vec<_> = records
            .iter()
            .map(|((entity, id), r)| json!({"entity":entity,"id":id,"revision":r.revision}))
            .collect();
        record_revisions.sort_by_key(|r| {
            (
                r["entity"].as_str().unwrap().to_owned(),
                r["id"].as_str().unwrap().to_owned(),
            )
        });
        // Fixed phase order, independent of merchant JSON ordering. Each phase sees the preceding result.
        for kind in ["price", "discount", "shipping", "validation"] {
            if !contract.hooks.iter().any(|h| h == kind) {
                continue;
            }
            let snap = snapshot(c, ps, &q, records.clone())?;
            let hook = match kind {
                "price" => wit::Kind::Price,
                "discount" => wit::Kind::Discount,
                "shipping" => wit::Kind::Shipping,
                _ => wit::Kind::Validation,
            };
            let input=json!({"taxStatus":q["price"]["taxStatus"],"salesChannel":c.data.sales_channel,"customerGroup":c.data.group,"shippingCosts":q["shippingCosts"]}).to_string();
            let active_guest = guest.clone();
            let out =
                tokio::task::spawn_blocking(move || active_guest.evaluate(snap, hook, &input))
                    .await
                    .map_err(|_| bad("Commerce hook worker failed"))?
                    .map_err(bad)?;
            if !out.accepted {
                return Err(bad(format!(
                    "Commerce hook {} rejected the cart: {}",
                    m.id, out.reason_code
                )));
            }
            commerce::app_adjustment(&mut q, kind, out.adjustment.minor)?;
            if kind != "validation" {
                q = commerce::enrich(q, c, ps, s, revision)?;
            }
            audit.push(json!({"app":m.id,"version":m.version,"digest":approval::canonical_digest(&m),"hook":kind,"minor":out.adjustment.minor,"currency":out.adjustment.currency,"scale":out.adjustment.scale,"reasonCode":out.reason_code,"recordRevisions":record_revisions}));
        }
    }
    q["appHookOutcomes"] = json!(audit);
    Ok(q)
}
