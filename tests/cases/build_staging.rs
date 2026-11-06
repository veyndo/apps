//! Required staging host calls the actual shared installer; rejection/failure rolls back packages, versions, events and build state.
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use tokio::sync::Notify;
use veyndo_app_storage::{
    Scope,
    build_versions::{self, StageAdmission, StageError, StageFuture},
    installation,
};
struct Host {
    deny: bool,
    fail: &'static str,
    pause: Option<(Arc<Notify>, Arc<Notify>)>,
}
impl<'db> StageAdmission<'db> for Host {
    type Error = &'static str;
    fn install<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        environment: &'a str,
        manifest: &'a Value,
    ) -> StageFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            if let Some((entered, release)) = &self.pause {
                entered.notify_one();
                release.notified().await;
            }
            sqlx::query(
                "INSERT INTO outbox(tenant,kind,data) VALUES('version-stage-host','fixture','{}')",
            )
            .execute(&mut **tx)
            .await
            .map_err(|_| "host db")?;
            if self.deny || environment != "version-private" {
                return Err("current environment/rights denied");
            }
            let package = crate::installation::package(environment, manifest, true);
            installation::install(
                tx,
                &package,
                &crate::installation::Hooks { fail: self.fail },
            )
            .await
            .map_err(|_| "installer rejected")
        })
    }
}
fn host() -> Host {
    Host {
        deny: false,
        fail: "",
        pause: None,
    }
}
async fn state(pool: &PgPool, id: &str) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT state FROM developer_builds WHERE id=$1")
        .bind(id)
        .fetch_one(pool)
        .await
}
pub(super) async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let digest = super::save(
        pool,
        "version-stage",
        "example",
        "stage-one",
        "1.0.0",
        false,
    )
    .await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        build_versions::stage(&mut tx, "foreign", "stage-one", &json!(digest), &host()).await,
        Err(StageError::Unavailable)
    ));
    assert!(matches!(
        build_versions::stage(
            &mut tx,
            "version-stage",
            "stage-one",
            &json!("stale"),
            &host()
        )
        .await,
        Err(StageError::Conflict("Build digest changed"))
    ));
    tx.rollback().await?;
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(pool)
        .await?;
    for reject in ["rights", "prepare", "admit", "attach", "finish"] {
        let mut tx = pool.begin().await?;
        let mut h = host();
        if reject == "rights" {
            h.deny = true;
        } else {
            h.fail = reject;
        }
        assert!(matches!(
            build_versions::stage(&mut tx, "version-stage", "stage-one", &json!(digest), &h).await,
            Err(StageError::Host(_))
        ));
        tx.rollback().await?;
        assert_eq!(state(pool, "stage-one").await?, "draft");
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM app_packages WHERE tenant='version-private'"
            )
            .fetch_one(pool)
            .await?,
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM app_versions WHERE tenant='version-private'"
            )
            .fetch_one(pool)
            .await?,
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
                .fetch_one(pool)
                .await?,
            before
        );
    }
    let mut tx = pool.begin().await?;
    let result = build_versions::stage(
        &mut tx,
        "version-stage",
        "stage-one",
        &json!(digest),
        &host(),
    )
    .await
    .unwrap();
    assert_eq!(result["installed"], true);
    assert_eq!(result["id"], "example");
    tx.commit().await?;
    assert_eq!(state(pool, "stage-one").await?, "staged");
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_versions WHERE tenant='version-private'"
        )
        .fetch_one(pool)
        .await?,
        1
    );
    let mut tx = pool.begin().await?;
    build_versions::archive(
        &mut tx,
        Scope {
            tenant: "version-stage",
            app: "example",
        },
        true,
    )
    .await?;
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        build_versions::stage(
            &mut tx,
            "version-stage",
            "stage-one",
            &json!(digest),
            &host()
        )
        .await,
        Err(StageError::Unavailable)
    ));
    tx.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_versions WHERE tenant='version-private'"
        )
        .fetch_one(pool)
        .await?,
        1
    );
    let mut tx = pool.begin().await?;
    build_versions::archive(
        &mut tx,
        Scope {
            tenant: "version-stage",
            app: "example",
        },
        false,
    )
    .await?;
    tx.commit().await?;
    // A failure after the installer also rolls back its complete writes.
    let bad = super::save(
        pool,
        "version-stage",
        "another",
        "version-state-fail",
        "1.0.0",
        false,
    )
    .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(pool)
        .await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        build_versions::stage(
            &mut tx,
            "version-stage",
            "version-state-fail",
            &json!(bad),
            &host()
        )
        .await,
        Err(StageError::Database(_))
    ));
    tx.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_packages WHERE tenant='version-private' AND id='another'"
        )
        .fetch_one(pool)
        .await?,
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
            .fetch_one(pool)
            .await?,
        count
    );
    // Archive must wait for the same locked build row while staging is active.
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let h = Host {
        deny: false,
        fail: "",
        pause: Some((entered.clone(), release.clone())),
    };
    let cloned = pool.clone();
    let task = tokio::spawn(async move {
        let mut tx = cloned.begin().await.unwrap();
        let result =
            build_versions::stage(&mut tx, "version-stage", "stage-one", &json!(digest), &h)
                .await
                .unwrap();
        tx.commit().await.unwrap();
        result
    });
    entered.notified().await;
    let cloned = pool.clone();
    let mut archive = tokio::spawn(async move {
        let mut tx = cloned.begin().await.unwrap();
        let n = build_versions::archive(
            &mut tx,
            Scope {
                tenant: "version-stage",
                app: "example",
            },
            true,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        n
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut archive)
            .await
            .is_err()
    );
    release.notify_one();
    assert_eq!(task.await?["installed"], true);
    assert_eq!(archive.await?, 1);
    println!(
        "PASS version staging: required current host admission, actual shared installer, foreign/digest/archive denial, failure rollback and archive/stage row contention"
    );
    Ok(())
}
