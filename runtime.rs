//! Generic pure-Wasm contribution executor; the installed package supplies all business predicates.
use super::*;
use std::sync::OnceLock;
pub(crate) async fn contribution(c: &ConfigurationContract, fee: i64, length: i64) -> Result<i64> {
    execute(c, fee, Some(length)).await
}
pub(crate) async fn validate_fee(c: &ConfigurationContract, fee: i64) -> Result<i64> {
    execute(c, fee, None).await
}
async fn execute(c: &ConfigurationContract, fee: i64, length: Option<i64>) -> Result<i64> {
    static CACHE: OnceLock<crate::sandbox_cache::Cache> = OnceLock::new();
    let source = c.wasm_source.clone();
    let guest = CACHE
        .get_or_init(crate::sandbox_cache::Cache::default)
        .compile(&hash(&source), source)
        .await?;
    tokio::task::spawn_blocking(move || {
        let result = if let Some(length) = length {
            guest.contribution(fee, length).map_err(bad)?
        } else if guest.validate_contribution(fee).map_err(bad)? {
            fee
        } else {
            return Err(bad("App rejected price data"));
        };
        if !(0..=100_000_000).contains(&result) {
            return Err(bad(
                "App rejected configuration or exceeded platform money bound",
            ));
        }
        Ok(result)
    })
    .await
    .map_err(|_| bad("Configuration runtime failed"))?
}
