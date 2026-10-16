//! Configuration mutations publish minimized identifiers through the native outbox/Flows; no cart capability or shopper text is disclosed.
use super::*;
pub(crate) fn configuration_budget(configs: &HashMap<String, Configuration>) -> Result<()> {
    if configs.len() > 100 || json!(configs).to_string().len() > 65536 {
        return Err(bad(
            "Cart configuration exceeds its 100-selection or 64 KiB budget",
        ));
    }
    Ok(())
}
pub(crate) async fn configuration_events(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    c: &StoredCart,
    before: &HashMap<String, Configuration>,
) -> Result<()> {
    let mut apps = std::collections::BTreeSet::new();
    for config in before.values().chain(c.data.app_configurations.values()) {
        apps.insert(config.app.as_str());
    }
    for app in apps {
        let old = before
            .iter()
            .filter(|(_, v)| v.app == app)
            .collect::<std::collections::BTreeMap<_, _>>();
        let new = c
            .data
            .app_configurations
            .iter()
            .filter(|(_, v)| v.app == app)
            .collect::<std::collections::BTreeMap<_, _>>();
        if json!(old) == json!(new) {
            continue;
        }
        let products = new
            .values()
            .map(|v| &v.product_id)
            .collect::<std::collections::BTreeSet<_>>();
        sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES($1,'app.configuration.changed',$2)")
            .bind(&c.tenant).bind(json!({"app":app,"cartId":c.id,"revision":c.revision,"salesChannelId":c.data.sales_channel,"operation":if new.is_empty() {"removed"} else {"configured"},"products":products}))
            .execute(&mut **tx).await?;
    }
    Ok(())
}
