//! Mandatory synthetic validation and installation hooks for the real schema recovery transaction tests.
use super::*;
pub(super) struct Policy {
    pub tenant: &'static str,
    pub deny: bool,
}
impl<'db> Validation<'db> for Policy {
    type Error = String;
    fn fields<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'db, sqlx::Postgres>,
        _: &'a Entity,
        fields: &'a Value,
    ) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let tenant: String = sqlx::query_scalar("SELECT current_setting('rac.tenant')")
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| e.to_string())?;
            if tenant != self.tenant || self.deny {
                return Err("fixture host language policy denied".into());
            }
            if !fields["count"].is_i64()
                || fields["active"] != true
                || !fields["caption"].is_object()
            {
                return Err("fixture typed fields denied".into());
            }
            Ok(())
        })
    }
}
pub(super) struct Host<'a> {
    pub(super) tenant: &'static str,
    pub(super) manifest: &'a Manifest,
    pub(super) deny: bool,
    pub(super) fail_finish: bool,
}
impl<'db> InstallHooks<'db> for Host<'_> {
    type Error = String;
    fn prepare<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            // A real host supplies full package compiler, current identity and signed scope here.
            sqlx::query("SELECT set_config('fixture.schema_host','admitted',true)")
                .execute(&mut **tx)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }
    fn admit<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let admitted: String =
                sqlx::query_scalar("SELECT current_setting('fixture.schema_host')")
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(|e| e.to_string())?;
            if admitted != "admitted" {
                return Err("fixture admission absent".into());
            }
            Ok(())
        })
    }
    fn upgrade<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        previous: &'a Value,
    ) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let old = serde_json::from_value(previous.clone()).map_err(|e| e.to_string())?;
            schema_upgrade::upgrade(
                tx,
                Scope {
                    tenant: self.tenant,
                    app: &self.manifest.id,
                },
                &old,
                self.manifest,
                &Policy {
                    tenant: self.tenant,
                    deny: self.deny,
                },
            )
            .await
            .map_err(|e| format!("{e:?}"))
        })
    }
    fn attach<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            for e in &self.manifest.entities {
                records::define(
                    tx,
                    Scope {
                        tenant: self.tenant,
                        app: &self.manifest.id,
                    },
                    &self.manifest.version,
                    &e.name,
                    &json!(e),
                )
                .await
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }
    fn finish<'a>(&'a self, _: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, String>
    where
        'db: 'a,
    {
        Box::pin(async move {
            if self.fail_finish {
                Err("fixture late installation failure".into())
            } else {
                Ok(())
            }
        })
    }
}
