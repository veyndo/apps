//! Required synthetic host reads durable management/consent/trust/commerce facts and connected references; production uses native owners instead.
use super::*;
use sqlx::{Postgres, Transaction};
use veyndo_app_storage::{activation::StateHooks, installation::HookFuture};
pub(super) struct Host<'a> {
    pub scope: Scope<'a>,
    pub fail: &'static str,
}
impl Host<'_> {
    async fn audit(&self, tx: &mut Transaction<'_, Postgres>, phase: &str) -> Result<(), String> {
        sqlx::query("INSERT INTO activation_audit(tenant,app,phase) VALUES($1,$2,$3)")
            .bind(self.scope.tenant)
            .bind(self.scope.app)
            .bind(phase)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        if self.fail == phase {
            return Err(format!("denied {phase}"));
        }
        Ok(())
    }
}
impl<'db> StateHooks<'db> for Host<'_> {
    type Error = String;
    fn prepare<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let allowed: Option<bool> = sqlx::query_scalar(
                "SELECT can_manage FROM activation_hosts WHERE tenant=$1 FOR UPDATE",
            )
            .bind(self.scope.tenant)
            .fetch_optional(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
            if allowed != Some(true) {
                return Err("current management denied".into());
            }
            self.audit(tx, "prepare").await
        })
    }
    fn activating<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let row=sqlx::query("SELECT consent_ok,trust_ok,commerce_ok,lifecycle_ok FROM app_packages p JOIN activation_facts f ON f.tenant=p.tenant AND f.app=p.id WHERE p.tenant=$1 AND p.id=$2")
                .bind(self.scope.tenant).bind(self.scope.app).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?.ok_or("package unavailable")?;
            for name in ["consent_ok", "trust_ok", "commerce_ok", "lifecycle_ok"] {
                if !row.get::<bool, _>(name) {
                    return Err(format!("current {name} denied"));
                }
            }
            self.audit(tx, "activating").await
        })
    }
    fn deactivating<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let used: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM activation_frontends WHERE tenant=$1 AND app=$2)",
            )
            .bind(self.scope.tenant)
            .bind(self.scope.app)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
            if used {
                return Err("connected frontend".into());
            }
            self.audit(tx, "deactivating").await
        })
    }
}
