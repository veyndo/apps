//! Executable immutable version/history recovery and mandatory staging authority boundary; disposable fixture rolls back fully.
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use veyndo_app_storage::{
    Scope,
    build_versions::{self, Build, StageAdmission, StageError, StageFuture},
};
struct NoInstaller;
impl<'db> StageAdmission<'db> for NoInstaller {
    type Error = &'static str;
    fn install<'a>(
        &'a self,
        _: &'a mut Transaction<'db, Postgres>,
        _: &'a str,
        _: &'a Value,
    ) -> StageFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async {
            Err("Configure a real authorized environment/compiler/installer before staging")
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let tenant = format!(
        "versions-example-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let scope = Scope {
        tenant: &tenant,
        app: "example",
    };
    let manifest = json!({"id":"example","version":"1.0.0"});
    let mut tx = pool.begin().await?;
    let digest = build_versions::save(
        &mut tx,
        &Build {
            scope,
            id: &tenant,
            environment: "private-fixture",
            version: "1.0.0",
            manifest: &manifest,
            prompt: "synthetic instruction",
            provider: "fixture",
            model: None,
            summary: &json!({"en":"Synthetic draft"}),
        },
    )
    .await?;
    assert_eq!(
        build_versions::list(&mut tx, &tenant, false).await?.len(),
        1
    );
    assert_eq!(build_versions::archive(&mut tx, scope, true).await?, 1);
    assert!(
        build_versions::list(&mut tx, &tenant, false)
            .await?
            .is_empty()
    );
    assert_eq!(build_versions::list(&mut tx, &tenant, true).await?.len(), 1);
    assert!(matches!(
        build_versions::stage(&mut tx, &tenant, &tenant, &json!(digest), &NoInstaller).await,
        Err(StageError::Unavailable)
    ));
    assert_eq!(build_versions::archive(&mut tx, scope, false).await?, 1);
    assert!(matches!(
        build_versions::stage(&mut tx, &tenant, &tenant, &json!(digest), &NoInstaller).await,
        Err(StageError::Host(_))
    ));
    tx.rollback().await?;
    println!(
        "PASS independent version save/history/archive/restore and denied staging without an authorized installer; fixture rolled back"
    );
    Ok(())
}
