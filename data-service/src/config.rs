//! Operator configuration only; serving credentials cannot install schemas or mint system scope.
use veyndo_app_authority::{database::TenantSigner, oauth_config::Config as Identity};
pub struct Config {
    pub identity: Identity,
    pub database: zeroize::Zeroizing<String>,
    pub signer: TenantSigner,
    pub writes: bool,
    pub installs: Option<Vec<String>>,
    pub settings: Option<veyndo_app_authority::vault::Vault>,
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
        let writes = match std::env::var("APP_DATA_MODE").as_deref().unwrap_or("read") {
            "read" => false,
            "write" => true,
            _ => return Err("APP_DATA_MODE must be read or write"),
        };
        let settings = match std::env::var("APP_DATA_SETTINGS")
            .as_deref()
            .unwrap_or("disabled")
        {
            "disabled" => None,
            "enabled" if writes => {
                let raw = zeroize::Zeroizing::new(env("APP_DATA_SETTINGS_KEY")?);
                if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("APP_DATA_SETTINGS_KEY must be 64 hexadecimal characters");
                }
                let bytes = zeroize::Zeroizing::new(
                    (0..64)
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| "Invalid settings key")?,
                );
                Some(
                    veyndo_app_authority::vault::Vault::new(&bytes)
                        .map_err(|_| "Invalid settings key")?,
                )
            }
            _ => return Err("Settings require APP_DATA_MODE=write and APP_DATA_SETTINGS=enabled"),
        };
        let installs = match std::env::var("APP_DATA_INSTALLS")
            .as_deref()
            .unwrap_or("disabled")
        {
            "disabled" => None,
            "enabled" if settings.is_some() => {
                let scopes: Vec<String> = serde_json::from_str(&env("APP_DATA_INSTALL_SCOPES")?)
                    .map_err(|_| "Invalid installer scope catalogue")?;
                if scopes.is_empty()
                    || scopes.len() > 128
                    || scopes.iter().any(|s| {
                        s.is_empty()
                            || s.len() > 64
                            || !s.bytes().all(|b| {
                                b.is_ascii_lowercase()
                                    || b.is_ascii_digit()
                                    || b == b'.'
                                    || b == b'_'
                            })
                    })
                {
                    return Err("Invalid installer scope catalogue");
                }
                Some(scopes)
            }
            _ => return Err("Installation requires the explicitly enabled settings/write profile"),
        };
        Ok(Self {
            installs,
            settings,
            writes,
            identity,
            database: zeroize::Zeroizing::new(env("APP_DATA_DATABASE_URL")?),
            signer,
        })
    }
}
