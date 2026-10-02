//! Generic pure-Wasm contribution executor; the installed package supplies all business predicates.
use super::*;
use std::sync::{Mutex, OnceLock};
pub(crate) async fn contribution(c: &ConfigurationContract, fee: i64, length: i64) -> Result<i64> {
    execute(c, fee, Some(length)).await
}
pub(crate) async fn validate_fee(c: &ConfigurationContract, fee: i64) -> Result<i64> {
    execute(c, fee, None).await
}
async fn execute(c: &ConfigurationContract, fee: i64, length: Option<i64>) -> Result<i64> {
    let source = c.wasm_source.clone();
    tokio::task::spawn_blocking(move || {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<Sandbox>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let key = hash(&source);
        let guest = {
            let mut cache = cache
                .lock()
                .map_err(|_| bad("Configuration cache unavailable"))?;
            if let Some(v) = cache.get(&key) {
                v.clone()
            } else {
                let v = Arc::new(Sandbox::new(&source).map_err(bad)?);
                if cache.len() >= 64 {
                    cache.clear()
                }
                cache.insert(key, v.clone());
                v
            }
        };
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
