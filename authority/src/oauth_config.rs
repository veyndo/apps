//! Operator-only immutable identity origin and bind configuration; no credentials or URLs are taken from requests.
use std::net::SocketAddr;
use zeroize::Zeroizing;
pub struct Config {
    pub introspection_url: reqwest::Url,
    pub introspection_credential: Zeroizing<String>,
    pub audience: String,
    pub bind: SocketAddr,
    pub allowed_origins: Vec<String>,
}
impl Config {
    pub fn new(
        endpoint: &str,
        credential: String,
        audience: String,
        bind: SocketAddr,
        allowed_origins: Vec<String>,
    ) -> Result<Self, &'static str> {
        let url = reqwest::Url::parse(endpoint).map_err(|_| "Invalid identity endpoint")?;
        let local = matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
        if (url.scheme() != "https"
            && !(local && url.scheme() == "http" && bind.ip().is_loopback()))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Identity endpoint requires HTTPS; local HTTP requires a loopback server bind",
            );
        }
        if credential.len() < 32
            || credential.len() > 4096
            || !credential.bytes().all(|b| b.is_ascii_graphic())
            || audience.is_empty()
            || audience.len() > 200
            || allowed_origins.len() > 16
        {
            return Err("Invalid identity credential, audience or origin count");
        }
        for origin in &allowed_origins {
            let u = reqwest::Url::parse(origin).map_err(|_| "Invalid allowed origin")?;
            if *origin != u.origin().ascii_serialization()
                || (u.scheme() != "https"
                    && !(bind.ip().is_loopback()
                        && matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))
                        && u.scheme() == "http"))
            {
                return Err("Allowed origins must be exact HTTPS origins");
            }
        }
        Ok(Self {
            introspection_url: url,
            introspection_credential: Zeroizing::new(credential),
            audience,
            bind,
            allowed_origins,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_origins_never_admit_credentials_redirects_or_insecure_remote_identity() {
        let bind = "127.0.0.1:0".parse().unwrap();
        for endpoint in [
            "http://identity.example/introspect",
            "https://user:pass@identity.example/introspect",
            "https://identity.example/introspect?token=private",
            "https://identity.example/introspect#private",
            "file:///tmp/key",
        ] {
            assert!(Config::new(endpoint, "x".repeat(32), "runtime".into(), bind, vec![]).is_err());
        }
        assert!(
            Config::new(
                "http://127.0.0.1:1/introspect",
                "x".repeat(32),
                "runtime".into(),
                "0.0.0.0:8798".parse().unwrap(),
                vec![]
            )
            .is_err()
        );
        for origin in [
            "*",
            "null",
            "https://example.test/path",
            "https://user:pass@example.test",
            "http://identity.example",
        ] {
            assert!(
                Config::new(
                    "https://identity.example/introspect",
                    "x".repeat(32),
                    "runtime".into(),
                    bind,
                    vec![origin.into()]
                )
                .is_err()
            );
        }
        assert!(
            Config::new(
                "https://identity.example/introspect",
                "x".repeat(32),
                "runtime".into(),
                bind,
                vec!["https://example.test".into()]
            )
            .is_ok()
        );
    }
}
