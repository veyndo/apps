//! Generic channel/product projections and imported sales delegate to native owners with explicit app capabilities.
use super::*;
pub(super) async fn execute(
    a: &App,
    h: &RequestContext,
    m: &Manifest,
    op: &str,
    v: &Value,
) -> Result<Value> {
    let t = merchant(a, h)?;
    let required = if op == "order_import" {
        vec![
            "orders.import",
            "payments.record",
            "orders.read",
            "customers.pii",
        ]
    } else {
        vec!["channels.read", "products.read"]
    };
    for p in required {
        if !m.permissions.iter().any(|q| q == p) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "App exchange permission not declared".into(),
            ));
        }
        credentials::permit(h, p)?;
    }
    if op == "order_import" {
        return commerce::import_external(a, h, &m.id, &approval::canonical_digest(m), v.clone())
            .await;
    }
    input_schema::validate(
        &json!({"type":"object","properties":{"id":{"type":"string","maxLength":100},"channelId":{"type":"string","maxLength":100},"currency":{"type":"string","maxLength":3}},"required":["channelId"],"additionalProperties":false}),
        v,
    )?;
    let id = v["channelId"].as_str().unwrap();
    let row = sqlx::query("SELECT data,revision FROM sales_channels WHERE tenant=$1 AND id=$2")
        .bind(&t)
        .bind(id)
        .fetch_optional(&a.db)
        .await?
        .ok_or(Error(StatusCode::NOT_FOUND, "Channel unavailable".into()))?;
    let c: Value = row.get("data");
    if c["kind"] != "marketplace" || c["active"] != true || c["visibility"] == "private" {
        return Err(conflict("Marketplace channel must be active and public"));
    }
    let (cfg, rev) = commerce::scoped_config(a, &t, id).await?;
    let mut result = json!({"id":id,"revision":row.get::<i64,_>("revision"),"data":c,"mainLocale":cfg.main_locale,"currencies":cfg.currencies,"settingsRevision":rev});
    if op == "channel_product" {
        let pid = v["id"].as_str().ok_or(bad("Product ID required"))?;
        let row = sqlx::query("SELECT * FROM products WHERE tenant=$1 AND id=$2")
            .bind(&t)
            .bind(pid)
            .fetch_optional(&a.db)
            .await?
            .ok_or(Error(StatusCode::NOT_FOUND, "Product unavailable".into()))?;
        let p = product(&row);
        let visible:bool=sqlx::query_scalar("SELECT p.active AND (p.parent_id IS NULL OR EXISTS(SELECT 1 FROM products parent WHERE parent.tenant=p.tenant AND parent.id=p.parent_id AND parent.active)) AND NOT EXISTS(SELECT 1 FROM product_channel_visibility v WHERE v.tenant=p.tenant AND v.channel_id=$3 AND NOT v.visible AND (v.product_id=p.id OR v.product_id=p.parent_id)) FROM products p WHERE p.tenant=$1 AND p.id=$2").bind(&t).bind(pid).bind(id).fetch_one(&a.db).await?;
        let selected = c["productIds"]
            .as_array()
            .ok_or(bad("Invalid channel catalog"))?;
        if !visible
            || (!selected.is_empty()
                && !selected
                    .iter()
                    .any(|v| v == pid || p.parent_id.as_ref().is_some_and(|parent| v == parent)))
        {
            return Err(bad("Product unavailable in channel"));
        }
        let currency = cfg
            .currencies
            .selected(v["currency"].as_str().unwrap_or(""))?;
        result["product"] =
            crate::operations::invoke(a, h, "merchant.product.content", &json!({"id":pid})).await?;
        result["money"] = json!(currencies::amount(
            currencies::product_price(&p, &cfg.currencies, currency)?,
            &currency.code,
            currency.scale
        )?);
    }
    Ok(result)
}
