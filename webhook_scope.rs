//! Scope signed app ingress from its route before identity, tenant lifecycle and forced-RLS admission.
use super::*;
pub(crate) fn bind(path: &str, h: &mut RequestContext) -> Result<()> {
    let Some(rest) = path.strip_prefix("/webhooks/apps/") else {
        return Ok(());
    };
    let parts: Vec<_> = rest.split('/').collect();
    if parts.len() != 3 || !identifier(parts[1]) || !identifier(parts[2]) {
        return Err(bad("Invalid app webhook route"));
    }
    let shop = parts[0];
    if !(2..=48).contains(&shop.len())
        || shop.starts_with('-')
        || shop.ends_with('-')
        || !shop
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(bad("Invalid app webhook shop scope"));
    }
    if header(h, "x-tenant").is_some_and(|v| v != shop) {
        return Err(bad("Webhook route conflicts with request shop scope"));
    }
    h.insert(
        "x-tenant",
        shop.parse().map_err(|_| bad("Invalid webhook scope"))?,
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_wins_without_accepting_conflicting_context() {
        let mut h = RequestContext::default();
        bind("/webhooks/apps/second-shop/example_webhook/changed", &mut h).unwrap();
        assert_eq!(header(&h, "x-tenant"), Some("second-shop"));
        assert!(bind("/webhooks/apps/victim/example_webhook/changed", &mut h).is_err());
        assert!(bind("/webhooks/apps/../app/hook", &mut RequestContext::default()).is_err());
    }
}
