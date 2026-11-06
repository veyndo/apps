//! Operator configuration only; serving credentials cannot install schemas or mint system scope.
use veyndo_app_authority::{database::TenantSigner, oauth_config::Config as Identity};
pub struct Config {
    pub identity: Identity,
    pub database: zeroize::Zeroizing<String>,
    pub signer: TenantSigner,
}
impl Config {
    pub fn load() -> Result<Self, &'static str> {
        let env = |n| std::env::var(n).map_err(|_| "Missing App Data service configuration");
        let bind = std::env::var("APP_DATA_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8799".into())
            .parse()
            .map_err(|_| "Invalid server bind")?;
        let identity = Identity::new(
            &env("APP_DATA_INTROSPECTION_URL")?,
            env("APP_DATA_INTROSPECTION_TOKEN")?,
            env("APP_DATA_AUDIENCE")?,
            bind,
            serde_json::from_str(
                &std::env::var("APP_DATA_ORIGINS").unwrap_or_else(|_| "[]".into()),
            )
            .map_err(|_| "Invalid origins")?,
        )?;
        let key = zeroize::Zeroizing::new(env("APP_DATA_SCOPE_KEY")?);
        let signer = TenantSigner::new(
            env("APP_DATA_SCOPE_KEY_ID")?,
            &key,
            env("APP_DATA_DATABASE_ROLE")?,
        )
        .map_err(|_| "Invalid database signing configuration")?;
        Ok(Self {
            identity,
            database: zeroize::Zeroizing::new(env("APP_DATA_DATABASE_URL")?),
            signer,
        })
    }
}
