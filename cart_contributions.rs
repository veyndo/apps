//! Generic app contributions: configure a cart, bind package/data revisions and persist audited pricing inputs.
use super::*;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Configuration {
    pub app: String,
    pub product_id: String,
    pub fields: Value,
    pub fee_minor: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_fields: Option<Value>,
    pub rule_revision: i64,
    pub app_version: String,
    pub entity: String,
    pub record: String,
}
pub(crate) async fn configure(a: &App, h: &RequestContext, id: &str, v: &Value) -> Result<Value> {
    let initial = load_cart(a, h).await?;
    let product = v["productId"].as_str().ok_or(bad("productId required"))?;
    marketing::admit_product(a, h, product).await?;
    if initial.status != "open" || v["revision"].as_i64() != Some(initial.revision) {
        return Err(conflict("Cart changed or terminal"));
    }
    let selection = configuration_selection::Selection {
        app: id.into(),
        product_id: product.into(),
        fields: v["fields"].clone(),
    };
    let config = configuration_selection::resolve_selection(a, &initial.tenant, &selection).await?;
    let mut candidate = initial.clone();
    candidate.data.app_configurations.remove(product);
    candidate
        .data
        .app_configurations
        .insert(format!("{product}:{id}"), config);
    configuration_budget(&candidate.data.app_configurations)?;
    let (settings, _) =
        commerce::scoped_config(a, &candidate.tenant, &candidate.data.sales_channel).await?;
    let currency = settings.currencies.selected(&candidate.data.currency)?;
    for configuration in candidate.data.app_configurations.values() {
        crate::cart_price::configuration_price(configuration, &settings.currencies, currency)?;
    }
    let modules = prepare_configurations(a, &candidate).await?;
    let mut tx = a.db.begin().await?;
    let row = sqlx::query("SELECT * FROM carts WHERE tenant=$1 AND token=$2 FOR UPDATE")
        .bind(&initial.tenant)
        .bind(&initial.token)
        .fetch_one(&mut *tx)
        .await?;
    let mut cart = stored(&row)?;
    pos::guard_cart(h, &cart)?;
    b2b::guard_conn(&mut tx, h, &cart).await?;
    if cart.status != "open" || cart.revision != initial.revision {
        return Err(conflict("Cart changed or terminal"));
    }
    let before = cart.data.app_configurations.clone();
    cart.data.app_configurations = candidate.data.app_configurations;
    // Validate even a configuration selected before its product is added to the cart.
    let mut checked = cart.clone();
    if !checked.data.items.iter().any(|item| item.id == product) {
        checked.data.items.push(Item {
            id: product.into(),
            quantity: 1,
        });
    }
    validate_configurations(&mut tx, &checked, &modules).await?;
    cart.revision += 1;
    sqlx::query("UPDATE carts SET data=$1,revision=$2 WHERE id=$3")
        .bind(json!(cart.data))
        .bind(cart.revision)
        .bind(&cart.id)
        .execute(&mut *tx)
        .await?;
    configuration_events(&mut tx, &cart, &before).await?;
    tx.commit().await?;
    cart_json(a, &cart).await
}
pub(crate) async fn validate_configurations(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    c: &StoredCart,
    prepared: &HashMap<String, std::sync::Arc<Sandbox>>,
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
        if contract.currency != config.currency
            || m.version != config.app_version
            || contract.entity != config.entity
            || configuration_inputs::record(contract, &config.fields)? != config.record
        {
            return Err(conflict("App changed; configure again"));
        }
        let r = records::get(
            tx,
            &c.tenant,
            &config.app,
            &contract.entity,
            &config.record,
            false,
        )
        .await?
        .ok_or(conflict("Configuration rules missing"))?;
        let fee = runtime::configured_contribution(
            prepared
                .get(&hash(&contract.wasm_source))
                .cloned()
                .ok_or(conflict("Cart app modules changed; retry checkout"))?,
            contract,
            r[&contract.price_field]
                .as_i64()
                .ok_or(bad("App price missing"))?,
            &config.fields,
        )
        .await?;
        if r["revision"].as_i64().ok_or(bad("App revision missing"))? != config.rule_revision
            || fee != config.fee_minor
        {
            return Err(conflict("App price changed; configure again"));
        }
    }
    Ok(())
}
/// Compile before opening checkout locks. Strong Arc references survive cache eviction;
/// the locked validation above checks the exact current source digest again.
pub(crate) async fn prepare_configurations(
    a: &App,
    c: &StoredCart,
) -> Result<HashMap<String, std::sync::Arc<Sandbox>>> {
    let mut prepared = HashMap::new();
    for config in c.data.app_configurations.values() {
        let m = package(a, &c.tenant, &config.app, true).await?;
        let contract = m
            .configuration
            .as_ref()
            .ok_or(conflict("App configuration missing"))?;
        let digest = hash(&contract.wasm_source);
        if let std::collections::hash_map::Entry::Vacant(e) = prepared.entry(digest) {
            e.insert(runtime::prepare(contract).await?);
        }
    }
    Ok(prepared)
}
