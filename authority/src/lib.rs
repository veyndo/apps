//! Trusted host authority adapters; no identity issuer, user store, configuration environment or database pool creation.
#[cfg(feature = "database")]
pub mod database;
#[cfg(feature = "oauth")]
pub mod oauth;
#[cfg(feature = "oauth")]
pub mod oauth_config;
