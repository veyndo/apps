//! Role-bound database scope tickets; private signing material never enters requests or app contexts.
/// Exact native lease kinds. Unknown tasks remain denied in the Core task-local adapter.
#[derive(Clone, Debug, Default)]
pub enum Scope {
    #[default]
    Denied,
    Tenant(String),
    Personal {
        tenant: String,
        actor: String,
    },
    Actor {
        tenant: String,
        actor: String,
    },
    System,
}
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Signer {
    id: String,
    secret: zeroize::Zeroizing<Vec<u8>>,
    tenant_role: String,
    system_role: String,
}
impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseScopeSigner")
            .field("key_id", &self.id)
            .finish_non_exhaustive()
    }
}
impl Signer {
    pub async fn load(pool: &sqlx::PgPool, tenant_role: &str) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        let row =
            sqlx::query("SELECT k.*,session_user::text AS login FROM public.veyndo_scope_key() k")
                .fetch_one(pool)
                .await?;
        let configured: Option<String> = row.get("tenant_login");
        if configured.as_deref() != Some(tenant_role) {
            return Err(sqlx::Error::Protocol(
                "Database tenant role does not match configured authority".into(),
            ));
        }
        let secret = zeroize::Zeroizing::new(row.get::<String, _>("secret"));
        Self::new(
            row.get("id"),
            &secret,
            tenant_role.to_owned(),
            row.get("login"),
        )
    }
    pub fn new(
        id: String,
        secret: &str,
        tenant_role: String,
        system_role: String,
    ) -> Result<Self, sqlx::Error> {
        if system_role.is_empty() || tenant_role == system_role {
            return Err(sqlx::Error::Protocol(
                "Invalid database scope signing configuration".into(),
            ));
        }
        let mut signer = Self::tenant(id, secret, tenant_role)?;
        signer.system_role = system_role;
        Ok(signer)
    }
    fn tenant(id: String, secret: &str, tenant_role: String) -> Result<Self, sqlx::Error> {
        if id.len() != 32
            || !id.bytes().all(|b| b.is_ascii_hexdigit())
            || secret.len() != 64
            || !secret.bytes().all(|b| b.is_ascii_hexdigit())
            || tenant_role.is_empty()
        {
            return Err(sqlx::Error::Protocol(
                "Invalid database scope signing configuration".into(),
            ));
        }
        let secret = secret
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        Ok(Self {
            id,
            secret: zeroize::Zeroizing::new(secret),
            tenant_role,
            system_role: String::new(),
        })
    }
    pub fn ticket(&self, scope: &Scope) -> Result<String, sqlx::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| sqlx::Error::Protocol("Invalid database scope clock".into()))?
            .as_secs();
        self.at(scope, now + 300, uuid::Uuid::new_v4().simple().to_string())
    }
    fn at(&self, scope: &Scope, deadline: u64, nonce: String) -> Result<String, sqlx::Error> {
        let (kind, tenant, actor, role) = match scope {
            Scope::Denied => ("D", "", "", &self.tenant_role),
            Scope::Tenant(tenant) => ("T", tenant.as_str(), "", &self.tenant_role),
            Scope::Personal { tenant, actor } => {
                ("P", tenant.as_str(), actor.as_str(), &self.tenant_role)
            }
            Scope::Actor { tenant, actor } => {
                ("A", tenant.as_str(), actor.as_str(), &self.tenant_role)
            }
            Scope::System => ("S", "", "", &self.system_role),
        };
        if !identifier(tenant) || !identifier(actor) {
            return Err(sqlx::Error::Protocol(
                "Invalid database scope identity".into(),
            ));
        }
        let body = format!("v1:{}:{deadline}:{kind}:{tenant}:{actor}:{nonce}", self.id);
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret).unwrap();
        mac.update(format!("{body}:{role}").as_bytes());
        let signature = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok(format!("{body}:{signature}"))
    }
}
pub fn identifier(value: &str) -> bool {
    value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// Tenant-only host signer: no public system-ticket operation or system database connection.
pub struct TenantSigner(Signer);
impl TenantSigner {
    pub fn new(id: String, secret: &str, role: String) -> Result<Self, sqlx::Error> {
        if role.is_empty() {
            return Err(sqlx::Error::Protocol(
                "Tenant database role required".into(),
            ));
        }
        Ok(Self(Signer::tenant(id, secret, role)?))
    }
    pub fn actor_ticket(&self, tenant: &str, actor: &str) -> Result<String, sqlx::Error> {
        if tenant.is_empty() || actor.is_empty() {
            return Err(sqlx::Error::Protocol("Tenant and actor required".into()));
        }
        self.0.ticket(&Scope::Actor {
            tenant: tenant.into(),
            actor: actor.into(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn signer() -> Signer {
        Signer::new(
            "a".repeat(32),
            &"07".repeat(32),
            "tenant_role".into(),
            "system_role".into(),
        )
        .unwrap()
    }
    #[test]
    fn tenant_host_has_no_system_mode_and_requires_both_canonical_ids() {
        let s = TenantSigner::new("a".repeat(32), &"07".repeat(32), "reader".into()).unwrap();
        let ticket = s.actor_ticket("shop", "user").unwrap();
        assert!(ticket.contains(":A:shop:user:"));
        for (tenant, actor) in [
            ("", "user"),
            ("shop", ""),
            ("other:shop", "user"),
            ("shop", "x".repeat(65).as_str()),
        ] {
            assert!(s.actor_ticket(tenant, actor).is_err());
        }
    }
    #[test]
    fn tickets_bind_kind_tenant_actor_role_and_never_interpolate_credentials() {
        let signer = signer();
        let tenant = signer
            .at(&Scope::Tenant("shop".into()), 123, "b".repeat(32))
            .unwrap();
        let actor = signer
            .at(
                &Scope::Personal {
                    tenant: "shop".into(),
                    actor: "person".into(),
                },
                123,
                "b".repeat(32),
            )
            .unwrap();
        let system = signer.at(&Scope::System, 123, "b".repeat(32)).unwrap();
        assert_ne!(tenant.split(':').next_back(), actor.split(':').next_back());
        assert_ne!(tenant.split(':').next_back(), system.split(':').next_back());
        assert_eq!(tenant.split(':').count(), 8);
        assert!(!format!("{signer:?}").contains(&"07".repeat(32)));
        assert!(
            signer
                .ticket(&Scope::Tenant("x'; SET rac.system='on".into()))
                .is_err()
        );
        assert_ne!(
            signer.ticket(&Scope::System).unwrap(),
            signer.ticket(&Scope::System).unwrap()
        );
    }
}
