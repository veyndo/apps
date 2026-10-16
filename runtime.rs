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
pub(crate) async fn validate_fee(c: &ConfigurationContract, fee: i64) -> Result<i64> {
    run(
        prepare(c).await?,
        fee,
        None,
        c.inputs.as_ref().map_or(1, |inputs| inputs.arguments.len()),
    )
    .await
}
pub(crate) async fn configured_contribution(
    guest: Arc<Sandbox>,
    c: &ConfigurationContract,
    fee: i64,
    fields: &Value,
) -> Result<i64> {
    if guest.digest() != hash(&c.wasm_source) {
        return Err(conflict("App module changed; retry checkout"));
    }
    let args = configuration_inputs::arguments(c, fields)?;
    run(
        guest,
        fee,
        Some(args),
        c.inputs.as_ref().map_or(1, |inputs| inputs.arguments.len()),
    )
    .await
}
async fn run(
    guest: Arc<Sandbox>,
    fee: i64,
    arguments: Option<Vec<i64>>,
    arity: usize,
) -> Result<i64> {
    tokio::task::spawn_blocking(move || {
        let result = if let Some(arguments) = arguments {
            guest.contribution_args(fee, &arguments).map_err(bad)?
        } else if guest.validate_contribution_args(fee, arity).map_err(bad)? {
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
