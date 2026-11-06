//! Runtime environment adapter over the shared immutable OAuth host configuration.
pub use veyndo_app_authority::oauth_config::Config;
pub fn load() -> Result<Config, &'static str> {
    let env = |name| std::env::var(name).map_err(|_| "Missing runtime service configuration");
    let bind = std::env::var("APP_RUNTIME_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8798".into())
        .parse()
        .map_err(|_| "Invalid server bind")?;
    let origins = std::env::var("APP_RUNTIME_ORIGINS").unwrap_or_else(|_| "[]".into());
    Config::new(
        &env("APP_RUNTIME_INTROSPECTION_URL")?,
        env("APP_RUNTIME_INTROSPECTION_TOKEN")?,
        env("APP_RUNTIME_AUDIENCE")?,
        bind,
        serde_json::from_str(&origins).map_err(|_| "Invalid allowed origins")?,
    )
}
