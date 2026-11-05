//! Real concurrent installs retain the original tenant quota lock and shared-writer/package-upgrade exclusion.
use super::installation::{Hooks, package};
use serde_json::json;
use sqlx::PgPool;
use veyndo_app_storage::{
    Scope,
    installation::{self, InstallError},
    packages,
};
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("INSERT INTO app_packages(tenant,id,version,manifest,digest) SELECT 'install-full','p'||n,'1.0.0',jsonb_build_object('id','p'||n,'version','1.0.0'),'fixture' FROM generate_series(1,127)n").execute(pool).await?;
    let mut tasks = Vec::new();
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    for id in ["extra_a", "extra_b"] {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            let value = json!({"id":id,"version":"1.0.0"});
            let mut tx = pool.begin().await.unwrap();
            barrier.wait().await;
            let result = installation::install(
                &mut tx,
                &package("install-full", &value, true),
                &Hooks { fail: "" },
            )
            .await;
            match result {
                Ok(_) => {
                    tx.commit().await.unwrap();
                    true
                }
                Err(InstallError::Invalid(
                    "Maximum 128 registered app packages per shop; update an existing app",
                )) => {
                    tx.rollback().await.unwrap();
                    false
                }
                result => panic!("Unexpected concurrent install result: {result:?}"),
            }
        }));
    }
    assert_eq!(
        tasks.remove(0).await? as u8 + tasks.remove(0).await? as u8,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_packages WHERE tenant='install-full'"
        )
        .fetch_one(pool)
        .await?,
        128
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox WHERE tenant='install-full' AND kind='app.installed'"
        )
        .fetch_one(pool)
        .await?,
        1
    );
    let mut tx = pool.begin().await?;
    let update = json!({"id":"p1","version":"2.0.0"});
    installation::install(
        &mut tx,
        &package("install-full", &update, true),
        &Hooks { fail: "" },
    )
    .await
    .unwrap();
    tx.commit().await?;
    let mut held = pool.begin().await?;
    packages::lock(
        &mut held,
        Scope {
            tenant: "install-full",
            app: "p1",
        },
    )
    .await?;
    let mut upgrade = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='150ms'")
        .execute(&mut *upgrade)
        .await?;
    let update = json!({"id":"p1","version":"3.0.0"});
    let Err(InstallError::Database(error)) = installation::install(
        &mut upgrade,
        &package("install-full", &update, true),
        &Hooks { fail: "" },
    )
    .await
    else {
        panic!("Upgrade escaped writer fence")
    };
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
    upgrade.rollback().await?;
    held.rollback().await?;
    Ok(())
}
