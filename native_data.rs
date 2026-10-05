//! App record translations use configured shop languages and field-level main-language inheritance.
use super::*;
pub(super) async fn validate_languages(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    t: &str,
    e: &Entity,
    fields: &Value,
) -> Result<()> {
    let data: Value = sqlx::query_scalar("SELECT data FROM commerce_settings WHERE tenant=$1")
        .bind(t)
        .fetch_one(&mut **tx)
        .await?;
    let settings = commerce::decode_config(data)?;
    for f in e.fields.iter().filter(|f| f.translatable) {
        let Some(map) = fields[&f.name].as_object() else {
            continue;
        };
        if map.keys().any(|k| {
            !settings
                .locales
                .iter()
                .any(|l| l == k || l.split('-').next() == Some(k.as_str()))
        }) {
            return Err(bad("App translation language is not enabled for this shop"));
        }
        if f.required
            && fields[&f.name][&settings.main_locale]
                .as_str()
                .or_else(|| {
                    fields[&f.name][settings.main_locale.split('-').next().unwrap()].as_str()
                })
                .is_none()
        {
            return Err(bad("Required app field needs a main-language value"));
        }
    }
    Ok(())
}
