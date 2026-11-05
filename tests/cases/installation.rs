//! Independent installation uses required hooks and checks ordering, rollback, versions, preview history and tenant quotas under contention.
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use veyndo_app_storage::{
    Scope,
    installation::{self, InstallError, InstallHooks, Package},
};
pub(super) struct Hooks {
    pub(super) fail: &'static str,
}
impl Hooks {
    async fn stage(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO outbox(tenant,kind,data) VALUES('install-hook','fixture',$1)")
            .bind(json!({"stage":name}))
            .execute(&mut **tx)
            .await?;
        if self.fail == name {
            return Err(sqlx::Error::Protocol(format!("denied {name}")));
        }
        Ok(())
    }
}
impl<'db> InstallHooks<'db> for Hooks {
    type Error = sqlx::Error;
    fn prepare<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> installation::HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { self.stage(tx, "prepare").await })
    }
    fn admit<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> installation::HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { self.stage(tx, "admit").await })
    }
    fn upgrade<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        previous: &'a Value,
    ) -> installation::HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            assert!(previous["id"].is_string());
            self.stage(tx, "upgrade").await
        })
    }
    fn attach<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> installation::HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { self.stage(tx, "attach").await })
    }
    fn finish<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
    ) -> installation::HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { self.stage(tx, "finish").await })
    }
}
pub(super) fn package<'a>(tenant: &'a str, value: &'a Value, versions: bool) -> Package<'a> {
    Package {
        scope: Scope {
            tenant,
            app: value["id"].as_str().unwrap(),
        },
        version: value["version"].as_str().unwrap(),
        manifest: value,
        versions,
    }
}
async fn counts(pool: &PgPool, tenant: &str) -> Result<(i64, i64, i64), sqlx::Error> {
    sqlx::query_as("SELECT (SELECT count(*) FROM app_packages WHERE tenant=$1),(SELECT count(*) FROM app_versions WHERE tenant=$1),(SELECT count(*) FROM outbox WHERE tenant=$1 OR tenant='install-hook')")
        .bind(tenant).fetch_one(pool).await
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let hooks = Hooks { fail: "" };
    let first = json!({"id":"plugin","version":"1.0.0","permissions":["data.read"]});
    let next = json!({"id":"plugin","version":"1.1.0","permissions":["data.read","data.write"]});
    let mut tx = pool.begin().await?;
    let result = installation::install(&mut tx, &package("install-own", &first, true), &hooks)
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({"installed":true,"id":"plugin","version":"1.0.0","digest":veyndo_app_storage::packages::digest(&first)})
    );
    tx.commit().await?;
    assert_eq!(counts(pool, "install-own").await?, (1, 1, 5));
    let stages: Vec<Value> =
        sqlx::query_scalar("SELECT data FROM outbox WHERE tenant='install-hook' ORDER BY id")
            .fetch_all(pool)
            .await?;
    assert_eq!(
        stages,
        vec![
            json!({"stage":"prepare"}),
            json!({"stage":"admit"}),
            json!({"stage":"attach"}),
            json!({"stage":"finish"})
        ]
    );
    let mut tx = pool.begin().await?;
    installation::install(&mut tx, &package("install-own", &next, true), &hooks)
        .await
        .unwrap();
    tx.commit().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT revision FROM app_packages WHERE tenant='install-own'"
        )
        .fetch_one(pool)
        .await?,
        2
    );
    let baseline = counts(pool, "install-own").await?;
    let changed = json!({"id":"plugin","version":"1.0.0","permissions":["payments.record"]});
    let mut tx = pool.begin().await?;
    assert!(matches!(
        installation::install(&mut tx, &package("install-own", &changed, true), &hooks).await,
        Err(InstallError::Conflict("Published app version is immutable"))
    ));
    tx.rollback().await?;
    assert_eq!(counts(pool, "install-own").await?, baseline);
    for fail in ["prepare", "admit", "upgrade", "attach", "finish"] {
        let third = json!({"id":"plugin","version":"2.0.0"});
        let mut tx = pool.begin().await?;
        assert!(matches!(
            installation::install(
                &mut tx,
                &package("install-own", &third, true),
                &Hooks { fail }
            )
            .await,
            Err(InstallError::Hook(_))
        ));
        tx.rollback().await?;
        assert_eq!(counts(pool, "install-own").await?, baseline);
        assert_eq!(
            sqlx::query_scalar::<_, Value>(
                "SELECT manifest FROM app_packages WHERE tenant='install-own'"
            )
            .fetch_one(pool)
            .await?,
            next
        );
    }
    let mut tx = pool.begin().await?;
    let mut wrong = package("install-own", &first, true);
    wrong.scope.app = "foreign";
    assert!(matches!(
        installation::install(&mut tx, &wrong, &hooks).await,
        Err(InstallError::Invalid(_))
    ));
    tx.rollback().await?;
    assert_eq!(counts(pool, "install-own").await?, baseline);
    // Another shop owns the same version independently, with different content.
    let mut tx = pool.begin().await?;
    installation::install(&mut tx, &package("install-other", &changed, true), &hooks)
        .await
        .unwrap();
    tx.commit().await?;
    assert_eq!(
        sqlx::query_scalar::<_, Value>(
            "SELECT manifest FROM app_packages WHERE tenant='install-own'"
        )
        .fetch_one(pool)
        .await?,
        next
    );
    // Private mutable preview changes no immutable version ledger, even at the same version.
    let mut tx = pool.begin().await?;
    installation::install(
        &mut tx,
        &package("install-preview", &first, false),
        &Hooks { fail: "upgrade" },
    )
    .await
    .unwrap();
    installation::install(
        &mut tx,
        &package("install-preview", &changed, false),
        &Hooks { fail: "upgrade" },
    )
    .await
    .unwrap();
    tx.commit().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_versions WHERE tenant='install-preview'"
        )
        .fetch_one(pool)
        .await?,
        0
    );
    super::installation_concurrency::run(pool).await?;
    println!(
        "PASS installation: required hook order, published version immutability, rollback at all five host stages, own/foreign version identity, mutable preview without published history, package quota contention and write fence"
    );
    Ok(())
}
