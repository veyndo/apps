//! Trusted host authority adapters; no identity issuer, user store, configuration environment or database pool creation.
#[cfg(feature = "database")]
pub mod database;
#[cfg(feature = "oauth")]
pub mod oauth;
#[cfg(feature = "oauth")]
pub mod oauth_config;
#[cfg(feature = "vault")]
pub mod vault;

/// Exact common PostgreSQL verifier for explicit, migration-owner installation.
/// Hosts append their own fixed schema; serving processes never apply migrations.
#[cfg(feature = "database")]
pub const DATABASE_SCOPE_SCHEMA: &str = include_str!("../sql/signed_scope.sql");
