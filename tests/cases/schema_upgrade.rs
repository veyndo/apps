//! Real schema migration/recovery on the shared installer transaction; required fixture policy never grants production authority.
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use veyndo_app_storage::{
    RecordKey, Scope,
    installation::{self, HookFuture, InstallHooks},
    model::{Entity, manifest::Manifest},
    records,
    schema_upgrade::{self, UpgradeError, Validation},
};
#[path = "schema_upgrade_bounds.rs"]
mod bounds;
pub(super) fn manifests() -> (Manifest, Manifest) {
    let old: Manifest = serde_json::from_value(json!({"id":"schema_fixture","version":"1.0.0","coreApi":"v1","runtime":"declarative","permissions":[],"name":{"en":"Fixture"},"entities":[{"name":"entries","fields":[{"name":"title","kind":"string","translatable":true},{"name":"count","kind":"string"},{"name":"obsolete","kind":"string"}]}]})).unwrap();
    let mut value = json!(old);
    value["version"] = json!("1.1.0");
    value["entities"][0]["fields"] = json!([{"name":"caption","kind":"string","translatable":true},{"name":"count","kind":"integer"},{"name":"active","kind":"boolean","required":true}]);
    value["schemaMigrations"] = json!([{"fromVersion":"1.0.0","steps":[{"op":"rename","entity":"entries","field":"title","to":"caption"},{"op":"remove","entity":"entries","field":"obsolete"},{"op":"convert","entity":"entries","field":"count"},{"op":"fill","entity":"entries","field":"active","value":true}]}]);
    (old, serde_json::from_value(value).unwrap())
}
pub(super) struct Policy {
    pub tenant: &'static str,
    pub deny: bool,
}
impl Validation for Policy {
    type Error = String;
    fn fields<'a>(
        &'a self,
        tx: &'a mut sqlx::PgConnection,
        _: &'a Entity,
        fields: &'a Value,
    ) -> HookFuture<'a, String> {
        Box::pin(async move {
            let tenant: String = sqlx::query_scalar("SELECT current_setting('rac.tenant')")
                .fetch_one(&mut *tx)
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
struct Host<'a> {
    tenant: &'static str,
    manifest: &'a Manifest,
    deny: bool,
    fail_finish: bool,
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
pub(super) async fn setup(
    pool: &PgPool,
    tenant: &str,
    old: &Manifest,
    count: &str,
    revision: i64,
) -> Result<(), sqlx::Error> {
    let scope = Scope {
        tenant,
        app: &old.id,
    };
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO app_packages(tenant,id,manifest) VALUES($1,$2,$3)")
        .bind(tenant)
        .bind(&old.id)
        .bind(json!(old))
        .execute(&mut *tx)
        .await?;
    records::define(
        &mut tx,
        scope,
        &old.version,
        "entries",
        &json!(old.entities[0]),
    )
    .await?;
    records::put(&mut tx, RecordKey{scope,entity:"entries",id:"same"},revision,&json!({"title":{"de-DE":"Titel","en-GB":"Title","it-IT":"","pl-PL":null},"count":count,"obsolete":"recovery-only","undeclared":"must not be captured"})).await?;
    tx.commit().await
}
async fn install(
    pool: &PgPool,
    tenant: &'static str,
    m: &Manifest,
    deny: bool,
    fail_finish: bool,
) -> Result<(), String> {
    let value = json!(m);
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let package = super::installation::package(tenant, &value, true);
    let host = Host {
        tenant,
        manifest: m,
        deny,
        fail_finish,
    };
    let result = installation::install(&mut tx, &package, &host).await;
    match result {
        Ok(_) => tx.commit().await.map_err(|e| e.to_string()),
        Err(e) => {
            tx.rollback().await.map_err(|e| e.to_string())?;
            Err(format!("{e:?}"))
        }
    }
}
pub(super) async fn state(pool: &PgPool, tenant: &str, app: &str) -> Result<Value, sqlx::Error> {
    sqlx::query_scalar("SELECT jsonb_build_object('package',p.manifest,'revision',p.revision,'models',(SELECT jsonb_agg(to_jsonb(m) ORDER BY entity) FROM app_models m WHERE tenant=$1 AND app=$2),'rows',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM app_records r WHERE tenant=$1 AND app=$2),'history',(SELECT count(*) FROM app_schema_history WHERE tenant=$1 AND app=$2),'versions',(SELECT count(*) FROM app_versions WHERE tenant=$1 AND app=$2),'events',(SELECT count(*) FROM outbox WHERE tenant=$1)) FROM app_packages p WHERE tenant=$1 AND id=$2").bind(tenant).bind(app).fetch_one(pool).await
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let (old, next) = manifests();
    for (tenant, count, revision, deny, late, message) in [
        ("schema-bad-convert", "2.5", 1, false, false, "Model"),
        (
            "schema-denied",
            "42",
            1,
            true,
            false,
            "language policy denied",
        ),
        (
            "schema-overflow",
            "42",
            i64::MAX,
            false,
            false,
            "App record revision exhausted",
        ),
        (
            "schema-policy-order",
            "42",
            i64::MAX,
            true,
            false,
            "language policy denied",
        ),
        (
            "schema-late-failure",
            "42",
            1,
            false,
            true,
            "late installation failure",
        ),
    ] {
        setup(pool, tenant, &old, count, revision).await?;
        let before = state(pool, tenant, &old.id).await?;
        let failure = install(pool, tenant, &next, deny, late).await.unwrap_err();
        assert!(failure.contains(message), "{failure}");
        assert_eq!(state(pool, tenant, &old.id).await?, before);
    }
    for tenant in ["schema-success", "schema-foreign"] {
        setup(pool, tenant, &old, "42", 7).await?;
    }
    let mut other = old.clone();
    other.id = "another_app".into();
    setup(pool, "schema-success", &other, "999", 3).await?;
    let foreign = state(pool, "schema-foreign", &old.id).await?;
    let other_before = state(pool, "schema-success", &other.id).await?;
    install(pool, "schema-success", &next, false, false).await?;
    let after = state(pool, "schema-success", &old.id).await?;
    assert_eq!(after["package"], json!(next));
    assert_eq!(after["rows"][0]["revision"], 8);
    assert_eq!(
        after["rows"][0]["fields"],
        json!({"caption":{"de-DE":"Titel","en-GB":"Title","it-IT":"","pl-PL":null},"count":42,"active":true})
    );
    assert_eq!(after["history"], 1);
    assert_eq!(after["events"], 2);
    assert_eq!(after["versions"], 1);
    let backup: Value = sqlx::query_scalar(
        "SELECT records FROM app_schema_history WHERE tenant='schema-success' AND app=$1",
    )
    .bind(&old.id)
    .fetch_one(pool)
    .await?;
    assert_eq!(backup["entries"][0]["count"], "42");
    assert_eq!(backup["entries"][0]["obsolete"], "recovery-only");
    assert!(backup["entries"][0].get("undeclared").is_none());
    assert_eq!(backup["entries"][0]["revision"], 7);
    let previous: Value = sqlx::query_scalar(
        "SELECT old_manifest FROM app_schema_history WHERE tenant='schema-success' AND app=$1",
    )
    .bind(&old.id)
    .fetch_one(pool)
    .await?;
    assert_eq!(previous, json!(old));
    assert_eq!(state(pool, "schema-foreign", &old.id).await?, foreign);
    let other_after = state(pool, "schema-success", &other.id).await?;
    // Events are tenant-wide; every app-owned field remains unchanged.
    for field in [
        "package", "revision", "models", "rows", "history", "versions",
    ] {
        assert_eq!(other_after[field], other_before[field]);
    }
    let mut tx = pool.begin().await?;
    assert!(matches!(
        schema_upgrade::upgrade(
            &mut tx,
            Scope {
                tenant: "schema-success",
                app: "another_app"
            },
            &old,
            &next,
            &Policy {
                tenant: "schema-success",
                deny: false
            }
        )
        .await,
        Err(UpgradeError::Invalid(_))
    ));
    tx.rollback().await?;
    bounds::run(pool).await?;
    println!(
        "PASS schema recovery: actual installer, typed host denial, conversion/late failure rollback, overflow/order, scoped multilingual snapshots and quota boundaries"
    );
    Ok(())
}
