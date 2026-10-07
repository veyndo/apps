//! Operator-owned private origins permit isolated service networking without relaxing public egress.
use reqwest::Url;

pub(super) fn allowed(url: &Url, private_origins: &str) -> bool {
    let private_origin = private_origins.split(',').any(|entry| {
        Url::parse(entry.trim()).is_ok_and(|origin| {
            origin.scheme() == "http"
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.origin() == url.origin()
        })
    });
    crate::verified_kernel::app_service_transport_admissible(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        url.scheme() == "https",
        url.scheme() == "http",
        matches!(url.host_str(), Some("127.0.0.1" | "localhost")),
        private_origin,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn permits(url: &str, origins: &str) -> bool {
        allowed(&Url::parse(url).unwrap(), origins)
    }
    #[test]
    fn retains_secure_and_loopback_defaults() {
        assert!(permits("https://provider.example/app", ""));
        assert!(permits("http://127.0.0.1:8797/email", ""));
        assert!(!permits("http://vendune-connectors:8797/email", ""));
    }
    #[test]
    fn private_permission_matches_exact_scheme_host_and_port() {
        let origins = "http://vendune-connectors:8797";
        assert!(permits("http://vendune-connectors:8797/email", origins));
        for url in [
            "http://vendune-connectors:8798/email",
            "http://vendune-connectors.evil:8797/email",
            "http://other:8797/email",
            "ftp://vendune-connectors:8797/email",
        ] {
            assert!(!permits(url, origins));
        }
    }
    #[test]
    fn rejects_credentials_queries_fragments_and_non_origin_allowlist_entries() {
        for url in [
            "http://user@vendune-connectors:8797/email",
            "http://vendune-connectors:8797/email?token=secret",
            "https://provider.example/app#token",
        ] {
            assert!(!permits(url, "http://vendune-connectors:8797"));
        }
        for entry in [
            "*",
            "http://vendune-connectors:8797/email",
            "http://user@vendune-connectors:8797",
            "http://vendune-connectors:8797?x=1",
        ] {
            assert!(!permits("http://vendune-connectors:8797/email", entry));
        }
    }
}
