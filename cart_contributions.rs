//! Generic app contributions: configure a cart, bind package/data revisions and persist audited pricing inputs.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Configuration {
    pub app: String,
    pub product_id: String,
    pub fields: Value,
    pub fee_minor: i64,
    pub rule_revision: i64,
    pub app_version: String,
    pub entity: String,
    pub record: String,
}
pub(crate) async fn configure(a: &App, h: &HeaderMap, id: &str, v: &Value) -> Result<Value> {
    let t = tenant(h)?;
    let m = package(a, &t, id, true).await?;
    let contract = m.configuration.as_ref().ok_or(bad(
        "App has no configuration capability; upgrade its package",
    ))?;
    let input = v["fields"][&contract.input_field]
        .as_str()
        .filter(|s| s.len() <= 2000 && !s.chars().any(char::is_control))
        .ok_or(bad("Printable configuration field required"))?;
    if v["fields"].as_object().is_none_or(|o| o.len() != 1) {
        return Err(bad("Only declared configuration input is accepted"));
    }
    let product = v["productId"].as_str().ok_or(bad("productId required"))?;
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query("SELECT * FROM carts WHERE tenant=$1 AND token=$2 FOR UPDATE")
        .bind(&t)
        .bind(token(h)?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(bad("Cart not found"))?;
    let mut cart = stored(&row)?;
    if cart.status != "open" || v["revision"].as_i64() != Some(cart.revision) {
        return Err(conflict("Cart changed or terminal"));
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM products WHERE tenant=$1 AND id=$2)")
            .bind(&t)
            .bind(product)
            .fetch_one(&mut *tx)
            .await?;
    if !exists {
        return Err(bad("Unknown product"));
    }
    let sql = format!(
        "SELECT {},revision FROM public.{} WHERE tenant=$1 AND id=$2 FOR SHARE",
        contract.price_field,
        table(id, &contract.entity)
    );
    let rule = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&t)
        .bind(&contract.record)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(bad("App configuration data missing"))?;
    let fee = runtime::contribution(
        contract,
        rule.get(&*contract.price_field),
        input.chars().count() as i64,
    )
    .await?;
    cart.data.app_configurations.remove(product); // Replace a v1.0 legacy contribution for this SKU.
    cart.data.app_configurations.insert(
        format!("{product}:{id}"),
        Configuration {
            app: id.into(),
            product_id: product.into(),
            fields: v["fields"].clone(),
            fee_minor: fee,
            rule_revision: rule.get("revision"),
            app_version: m.version,
            entity: contract.entity.clone(),
            record: contract.record.clone(),
        },
    );
    cart.revision += 1;
    sqlx::query("UPDATE carts SET data=$1,revision=$2 WHERE id=$3")
        .bind(json!(cart.data))
        .bind(cart.revision)
        .bind(&cart.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    cart_json(a, &cart).await
}
pub(crate) async fn validate_configurations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    c: &StoredCart,
) -> Result<()> {
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&c.tenant)
        .execute(&mut **tx)
        .await?;
    for config in c
        .data
        .app_configurations
        .values()
        .filter(|v| c.data.items.iter().any(|i| i.id == v.product_id))
    {
        let row=sqlx::query("SELECT manifest,version FROM app_packages WHERE tenant=$1 AND id=$2 AND active FOR SHARE").bind(&c.tenant).bind(&config.app).fetch_optional(&mut **tx).await?.ok_or(conflict("Configuration app unavailable"))?;
        let m: Manifest =
            serde_json::from_value(row.get("manifest")).map_err(|_| bad("Invalid package"))?;
        let contract = m
            .configuration
            .as_ref()
            .ok_or(conflict("Upgrade and reconfigure this app"))?;
        if m.version != config.app_version
            || contract.entity != config.entity
            || contract.record != config.record
        {
            return Err(conflict("App changed; configure again"));
        }
        let sql = format!(
            "SELECT {},revision FROM public.{} WHERE tenant=$1 AND id=$2 FOR SHARE",
            contract.price_field,
            table(&config.app, &contract.entity)
        );
        let r = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
            .bind(&c.tenant)
            .bind(&config.record)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(conflict("Configuration rules missing"))?;
        let fee = runtime::contribution(
            contract,
            r.get(&*contract.price_field),
            config.fields[&contract.input_field]
                .as_str()
                .unwrap_or("")
                .chars()
                .count() as i64,
        )
        .await?;
        if r.get::<i64, _>("revision") != config.rule_revision || fee != config.fee_minor {
            return Err(conflict("App price changed; configure again"));
        }
    }
    Ok(())
}
pub(crate) async fn configurations(a: &App, t: &str, id: &str) -> Result<Value> {
    let rows=sqlx::query("SELECT id,data->'cart'->'lineItems' AS items FROM orders WHERE tenant=$1 AND data->'cart'->'lineItems' @? '$[*].configuration' ORDER BY created_at DESC LIMIT 100").bind(t).fetch_all(&a.db).await?;
    let mut orders = vec![];
    for r in rows {
        let items: Value = r.get("items");
        let items = items
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| {
                i["configuration"]["app"] == id
                    || i["appConfigurations"]
                        .as_array()
                        .is_some_and(|v| v.iter().any(|c| c["app"] == id))
            })
            .collect::<Vec<_>>();
        if !items.is_empty() {
            orders.push(json!({"orderId":r.get::<String,_>("id"),"items":items}));
        }
    }
    Ok(json!({"elements":orders}))
}
