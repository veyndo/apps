//! Engraving example: server-owned surcharge, typed cart data and checkout revision verification.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Configuration {
    pub text: String,
    pub fee_minor: i64,
    pub rule_revision: i64,
    pub app_version: String,
}
pub(crate) async fn configure(a: &App, h: &HeaderMap, v: &Value) -> Result<Value> {
    let t = tenant(h)?;
    let m = package(a, &t, "engraving", true).await?;
    let text = v["text"]
        .as_str()
        .filter(|s| !s.is_empty() && s.chars().count() <= 40 && !s.chars().any(char::is_control))
        .ok_or(bad("Engraving must contain 1..40 printable characters"))?;
    let product = v["productId"].as_str().ok_or(bad("productId required"))?;
    let mut tx = a.db.begin().await?;
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&t)
        .execute(&mut *tx)
        .await?;
    let r = sqlx::query("SELECT * FROM carts WHERE tenant=$1 AND token=$2 FOR UPDATE")
        .bind(&t)
        .bind(token(h)?)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(bad("Cart not found"))?;
    let mut c = stored(&r)?;
    if c.status != "open" || v["revision"].as_i64() != Some(c.revision) {
        return Err(conflict("Cart changed or is terminal"));
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
        "SELECT fee_minor,revision FROM public.{} WHERE tenant=$1 AND id='default' FOR SHARE",
        table("engraving", "rules")
    );
    let r = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&t)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(bad("Configure engraving fee in Apps first"))?;
    let fee = r.get::<i64, _>("fee_minor");
    if !(0..=100000).contains(&fee) {
        return Err(bad("Engraving fee out of range"));
    }
    c.data.app_configurations.insert(
        product.into(),
        Configuration {
            text: text.into(),
            fee_minor: fee,
            rule_revision: r.get("revision"),
            app_version: m.version,
        },
    );
    c.revision += 1;
    sqlx::query("UPDATE carts SET data=$1,revision=$2 WHERE id=$3")
        .bind(json!(c.data))
        .bind(c.revision)
        .bind(&c.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    cart_json(a, &c).await
}
pub(crate) async fn validate_configurations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    c: &StoredCart,
) -> Result<()> {
    if !c
        .data
        .items
        .iter()
        .any(|i| c.data.app_configurations.contains_key(&i.id))
    {
        return Ok(());
    }
    sqlx::query("SELECT set_config('rac.tenant',$1,true)")
        .bind(&c.tenant)
        .execute(&mut **tx)
        .await?;
    let version: Option<String> = sqlx::query_scalar(
        "SELECT version FROM app_packages WHERE tenant=$1 AND id='engraving' AND active FOR SHARE",
    )
    .bind(&c.tenant)
    .fetch_optional(&mut **tx)
    .await?;
    let sql = format!(
        "SELECT fee_minor,revision FROM public.{} WHERE tenant=$1 AND id='default' FOR SHARE",
        table("engraving", "rules")
    );
    let r = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&c.tenant)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(conflict("Engraving rules changed"))?;
    for i in &c.data.items {
        if let Some(config) = c.data.app_configurations.get(&i.id)
            && (version.as_deref() != Some(config.app_version.as_str())
                || r.get::<i64, _>("revision") != config.rule_revision
                || r.get::<i64, _>("fee_minor") != config.fee_minor)
        {
            return Err(conflict("App or engraving price changed; configure again"));
        }
    }
    Ok(())
}
pub(crate) async fn configurations(a: &App, t: &str) -> Result<Value> {
    let rows=sqlx::query("SELECT id,data->'cart'->'lineItems' AS items FROM orders WHERE tenant=$1 AND data->'cart'->'lineItems' @? '$[*].configuration' ORDER BY created_at DESC LIMIT 100").bind(t).fetch_all(&a.db).await?;
    Ok(
        json!({"elements":rows.iter().map(|r|json!({"orderId":r.get::<String,_>("id"),"items":r.get::<Value,_>("items")})).collect::<Vec<_>>()}),
    )
}
