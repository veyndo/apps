//! Bounded origin clients pin resolved addresses for each connection and never use ambient HTTP proxies.
use super::*;
use std::{
    net::SocketAddr,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
type Clients = HashMap<String, (Instant, reqwest::Client)>;
static CLIENTS: OnceLock<Mutex<Clients>> = OnceLock::new();
pub(super) async fn client(url: &reqwest::Url) -> Result<reqwest::Client> {
    connect(url, false).await
}
pub(super) async fn public_client(url: &reqwest::Url) -> Result<reqwest::Client> {
    connect(url, true).await
}
async fn connect(url: &reqwest::Url, public_only: bool) -> Result<reqwest::Client> {
    let origins = &crate::runtime_config::get().private_service_origins;
    if !service_policy::allowed(url, origins) || public_only && url.scheme() != "https" {
        return Err(bad(
            "App destination requires an approved HTTPS or private origin",
        ));
    }
    let key = format!("{public_only}:{}", url.origin().ascii_serialization());
    let cache = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((_, client)) = cache
        .lock()
        .map_err(|_| bad("App egress cache unavailable"))?
        .get(&key)
        .filter(|(at, _)| at.elapsed() < Duration::from_secs(60))
    {
        return Ok(client.clone());
    }
    let host = url.host_str().ok_or(bad("App hostname required"))?;
    let port = url
        .port_or_known_default()
        .ok_or(bad("App port required"))?;
    let addresses: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| {
        Error(
            StatusCode::BAD_GATEWAY,
            "App DNS resolution timed out".into(),
        )
    })?
    .map_err(|_| {
        Error(
            StatusCode::BAD_GATEWAY,
            "App hostname cannot be resolved".into(),
        )
    })?
    .collect();
    let loopback = !public_only && matches!(host, "localhost" | "127.0.0.1");
    // Operator approval is exact-origin, never a suffix/wildcard. Public destinations must reject
    // every private answer (including mixed public/private DNS), not just choose a public first entry.
    let private = !public_only
        && origins
            .split(',')
            .any(|s| reqwest::Url::parse(s.trim()).is_ok_and(|u| u.origin() == url.origin()));
    if addresses.is_empty()
        || addresses.len() > 16
        || !addresses.iter().all(|a| {
            private
                || if loopback {
                    a.ip().is_loopback()
                } else {
                    vendune::network_policy::public_address(a.ip())
                }
        })
    {
        return Err(Error(
            StatusCode::FORBIDDEN,
            "App destination resolves to a disallowed address".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(2))
        .pool_idle_timeout(Duration::from_secs(60))
        .build()
        .map_err(|_| bad("App transport unavailable"))?;
    let mut cache = cache
        .lock()
        .map_err(|_| bad("App egress cache unavailable"))?;
    cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
    if cache.len() >= 128
        && let Some(key) = cache.keys().next().cloned()
    {
        cache.remove(&key);
    }
    cache.insert(key, (Instant::now(), client.clone()));
    Ok(client)
}
