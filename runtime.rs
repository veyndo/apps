//! Generic pure-Wasm contribution executor; the installed package supplies all business predicates.
use super::*;
use std::sync::Arc;
use std::sync::OnceLock;
pub(crate) async fn prepare(c: &ConfigurationContract) -> Result<Arc<Sandbox>> {
    prepare_source(c.wasm_source.clone()).await
}
pub(crate) async fn prepare_source(source: String) -> Result<Arc<Sandbox>> {
    static CACHE: OnceLock<crate::sandbox_cache::Cache> = OnceLock::new();
    CACHE
        .get_or_init(crate::sandbox_cache::Cache::default)
        .compile(&hash(&source), source)
        .await
}
pub(crate) async fn prepared_contribution(
    guest: Arc<Sandbox>,
    c: &ConfigurationContract,
    fee: i64,
    length: i64,
) -> Result<i64> {
    if guest.digest() != hash(&c.wasm_source) {
        return Err(conflict("App module changed; retry checkout"));
    }
    run(guest, fee, Some(length)).await
}
pub(crate) async fn validate_fee(c: &ConfigurationContract, fee: i64) -> Result<i64> {
    execute(c, fee, None).await
}
async fn execute(c: &ConfigurationContract, fee: i64, length: Option<i64>) -> Result<i64> {
    let guest = prepare(c).await?;
    run(guest, fee, length).await
}
async fn run(guest: Arc<Sandbox>, fee: i64, length: Option<i64>) -> Result<i64> {
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
