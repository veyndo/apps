//! Actual portable installation hooks on the original shared sequence; native domains are denied, never approved by fallback hooks.
use crate::{
    database::unavailable,
    error::{Problem, Result},
    identity::Principal,
    install_policy, management, portable,
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use veyndo_app_storage::{
    self as store, Scope,
    installation::{HookFuture, InstallHooks},
    model::{self, manifest::Manifest},
};
pub fn conflict() -> Problem {
    Problem::new(
        StatusCode::CONFLICT,
        "package_conflict",
        "The installed package or version changed; review it again before installation",
    )
}
pub fn model_error<E>(e: store::models::Error<E>) -> Problem {
    match e {
        store::models::Error::Conflict(_) => conflict(),
        store::models::Error::Database(_) => unavailable(),
        _ => Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "model_incompatible",
            "The installed model cannot adopt this field change; use a supported migration host",
        ),
    }
}
pub struct Compatible;
impl store::models::Compatibility for Compatible {
    type Error = model::Error;
    fn check(&self, old: &model::Entity, next: &model::Entity) -> model::Result<()> {
        model::evolution::compatible(old, next)
    }
}
pub struct Host<'a> {
    pub principal: &'a Principal,
    pub manifest: &'a Manifest,
    pub revision: i64,
    pub scopes: &'a [String],
}
fn scope<'a>(p: &'a Principal, m: &'a Manifest) -> Scope<'a> {
    Scope {
        tenant: &p.tenant,
        app: &m.id,
    }
}
pub async fn prior(
    tx: &mut sqlx::PgConnection,
    p: &Principal,
    m: &Manifest,
) -> Result<(i64, Option<Value>)> {
    let row: Option<(i64, Value)> =
        sqlx::query_as("SELECT revision,manifest FROM app_packages WHERE tenant=$1 AND id=$2")
            .bind(&p.tenant)
            .bind(&m.id)
            .fetch_optional(tx)
            .await
            .map_err(|_| unavailable())?;
    Ok(row.map(|(r, v)| (r, Some(v))).unwrap_or((0, None)))
}
pub fn compatible(old: &Manifest, next: &Manifest) -> Result<()> {
    let version = |s: &str| {
        s.split('.')
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()
    };
    if version(&next.version).map_err(|_| conflict())?
        < version(&old.version).map_err(|_| conflict())?
    {
        return Err(conflict());
    }
    model::evolution::shape(&old.entities, &next.entities, None).map_err(|_| conflict())?;
    Ok(())
}
impl<'db> InstallHooks<'db> for Host<'_> {
    type Error = Problem;
    fn prepare<'a>(&'a self, _tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            management::admit(self.principal)?;
            install_policy::compile(json!(self.manifest), self.scopes)?;
            Ok(())
        })
    }
    fn admit<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let (revision, _) = prior(tx, self.principal, self.manifest).await?;
            if revision != self.revision || revision == i64::MAX {
                return Err(conflict());
            }
            management::admit(self.principal)
        })
    }
    fn upgrade<'a>(
        &'a self,
        _tx: &'a mut Transaction<'db, Postgres>,
        previous: &'a Value,
    ) -> HookFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let old = install_policy::compile(previous.clone(), self.scopes)?;
            compatible(&old, self.manifest)?;
            Ok(())
        })
    }
    fn attach<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO app_storage_usage(tenant,app) VALUES($1,$2) ON CONFLICT DO NOTHING",
            )
            .bind(&self.principal.tenant)
            .bind(&self.manifest.id)
            .execute(&mut **tx)
            .await
            .map_err(|_| unavailable())?;
            store::models::install(
                tx,
                scope(self.principal, self.manifest),
                self.manifest,
                &Compatible,
            )
            .await
            .map_err(model_error)
        })
    }
    fn finish<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Problem>
    where
        'db: 'a,
    {
        Box::pin(async move {
            for e in &self.manifest.entities {
                portable::check(self.manifest, e)?;
                let definition: Value = sqlx::query_scalar(
                    "SELECT definition FROM app_models WHERE tenant=$1 AND app=$2 AND entity=$3",
                )
                .bind(&self.principal.tenant)
                .bind(&self.manifest.id)
                .bind(&e.name)
                .fetch_one(&mut **tx)
                .await
                .map_err(|_| unavailable())?;
                if definition != json!(e) {
                    return Err(unavailable());
                }
            }
            Ok(())
        })
    }
}
