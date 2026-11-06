//! Independent immutable version snapshots: digest compatibility, scoped archive/history and concurrent non-overwrite.
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use veyndo_app_storage::{
    Error, Scope,
    build_versions::{self, Build},
};
#[path = "build_staging.rs"]
mod staging;
pub(super) async fn save(
    pool: &PgPool,
    tenant: &str,
    app: &str,
    id: &str,
    version: &str,
    extra: bool,
) -> Result<String, Error> {
    let mut tx = pool.begin().await?;
    let manifest = json!({"id":app,"version":version,"extra":extra});
    let digest = build_versions::save(
        &mut tx,
        &Build {
            scope: Scope { tenant, app },
            id,
            environment: "version-private",
            version,
            manifest: &manifest,
            prompt: "synthetic instruction",
            provider: "fixture",
            model: None,
            summary: &json!({"de":"Entwurf"}),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(digest)
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Scope {
        tenant: "version-own",
        app: "example",
    };
    let digest = save(pool, own.tenant, own.app, "version-one", "1.0.0", false).await?;
    let manifest = json!({"id":"example","version":"1.0.0","extra":false});
    use sha2::{Digest, Sha256};
    assert_eq!(
        digest,
        format!("{:x}", Sha256::digest(manifest.to_string().as_bytes()))
    );
    save(
        pool,
        "version-foreign",
        "example",
        "version-foreign-one",
        "1.0.0",
        false,
    )
    .await?;
    save(
        pool,
        own.tenant,
        "another",
        "version-other-app",
        "1.0.0",
        false,
    )
    .await?;
    assert!(matches!(
        save(pool, own.tenant, own.app, "changed", "1.0.0", true).await,
        Err(Error::Conflict(_))
    ));
    let mut tx = pool.begin().await?;
    let rows = build_versions::list(&mut tx, own.tenant, false).await?;
    assert_eq!(rows.len(), 2);
    let row = rows
        .iter()
        .find(|r| r.get::<String, _>("id") == "version-one")
        .unwrap();
    assert_eq!(row.get::<Value, _>("manifest"), manifest);
    assert_eq!(row.get::<String, _>("digest"), digest);
    assert_eq!(build_versions::archive(&mut tx, own, true).await?, 1);
    assert_eq!(build_versions::archive(&mut tx, own, true).await?, 0);
    assert_eq!(
        build_versions::list(&mut tx, own.tenant, false)
            .await?
            .len(),
        1
    );
    assert_eq!(
        build_versions::list(&mut tx, own.tenant, true).await?.len(),
        1
    );
    assert_eq!(
        build_versions::list(&mut tx, "version-foreign", false)
            .await?
            .len(),
        1
    );
    tx.commit().await?;
    assert!(matches!(
        save(pool, own.tenant, own.app, "archived-reuse", "1.0.0", true).await,
        Err(Error::Conflict(_))
    ));
    let mut tx = pool.begin().await?;
    assert_eq!(build_versions::archive(&mut tx, own, false).await?, 1);
    let result = build_versions::save(
        &mut tx,
        &Build {
            scope: own,
            id: "bad",
            environment: "version-private",
            version: "2",
            manifest: &json!({"id":"foreign","version":"2"}),
            prompt: "fixture",
            provider: "fixture",
            model: None,
            summary: &json!({}),
        },
    )
    .await;
    assert!(matches!(result, Err(Error::Invalid(_))));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert_eq!(
        build_versions::list(&mut tx, own.tenant, true).await?.len(),
        1
    );
    tx.rollback().await?;
    let mut tasks = vec![];
    for (id, extra) in [("race-build-a", false), ("race-build-b", true)] {
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            save(&pool, "version-race", "example", id, "1.0.0", extra).await
        }));
    }
    let left = tasks.remove(0).await?;
    let right = tasks.remove(0).await?;
    assert_ne!(left.is_ok(), right.is_ok());
    for result in [left, right] {
        if let Err(e) = result {
            assert!(matches!(e, Error::Conflict(_)));
        }
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM developer_builds WHERE tenant='version-race'")
            .fetch_one(pool)
            .await?;
    assert_eq!(count, 1);
    for i in 0..105 {
        save(
            pool,
            "version-page",
            "example",
            &format!("page-{i}"),
            &format!("1.0.{i}"),
            false,
        )
        .await?;
    }
    let mut tx = pool.begin().await?;
    assert_eq!(
        build_versions::list(&mut tx, "version-page", false)
            .await?
            .len(),
        100
    );
    assert_eq!(
        build_versions::archive(
            &mut tx,
            Scope {
                tenant: "version-page",
                app: "example"
            },
            true
        )
        .await?,
        105
    );
    assert_eq!(
        build_versions::list(&mut tx, "version-page", true)
            .await?
            .len(),
        100
    );
    tx.rollback().await?;
    staging::run(pool).await?;
    println!(
        "PASS version snapshots: legacy digest, tenant/app scope, bounded history, immutable archived identities, restore/rollback and concurrent save"
    );
    Ok(())
}
