//! Real installed-package locks distinguish scope, active state and upgrades while retaining canonical content fingerprints.
use serde_json::json;
use sqlx::PgPool;
use veyndo_app_storage::{Scope, packages};
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("CREATE TABLE app_packages(tenant text NOT NULL,id text NOT NULL,active boolean NOT NULL,manifest jsonb NOT NULL,PRIMARY KEY(tenant,id))").execute(pool).await?;
    let own = Scope {
        tenant: "package-own",
        app: "plugin",
    };
    let foreign = Scope {
        tenant: "package-foreign",
        app: "plugin",
    };
    let v1 = json!({"id":"plugin","version":"1","permissions":["data.read"]});
    let v2 = json!({"id":"plugin","version":"2","permissions":["data.read","data.write"]});
    assert_eq!(
        packages::digest(&json!({"b":2,"a":1})),
        packages::digest(&json!({"a":1,"b":2}))
    );
    assert_ne!(packages::digest(&v1), packages::digest(&v2));
    for scope in [own, foreign] {
        sqlx::query("INSERT INTO app_packages VALUES($1,$2,true,$3)")
            .bind(scope.tenant)
            .bind(scope.app)
            .bind(&v1)
            .execute(pool)
            .await?;
    }
    let mut held = pool.begin().await?;
    assert_eq!(packages::lock(&mut held, own).await?, Some(v1.clone()));
    let mut upgrade = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='150ms'")
        .execute(&mut *upgrade)
        .await?;
    let error =
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('package-own:plugin',7))")
            .execute(&mut *upgrade)
            .await
            .unwrap_err();
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
    upgrade.rollback().await?;
    let mut other = pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='150ms'")
        .execute(&mut *other)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('package-foreign:plugin',7))")
        .execute(&mut *other)
        .await?;
    sqlx::query("UPDATE app_packages SET manifest=$1 WHERE tenant=$2 AND id=$3")
        .bind(&v2)
        .bind(foreign.tenant)
        .bind(foreign.app)
        .execute(&mut *other)
        .await?;
    other.commit().await?;
    held.commit().await?;
    let mut upgrade = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('package-own:plugin',7))")
        .execute(&mut *upgrade)
        .await?;
    sqlx::query("UPDATE app_packages SET manifest=$1 WHERE tenant=$2 AND id=$3")
        .bind(&v2)
        .bind(own.tenant)
        .bind(own.app)
        .execute(&mut *upgrade)
        .await?;
    upgrade.commit().await?;
    let mut tx = pool.begin().await?;
    assert_eq!(packages::lock(&mut tx, own).await?, Some(v2));
    assert!(
        packages::lock(
            &mut tx,
            Scope {
                tenant: "missing",
                app: "plugin"
            }
        )
        .await?
        .is_none()
    );
    tx.rollback().await?;
    sqlx::query("UPDATE app_packages SET active=false WHERE tenant=$1")
        .bind(own.tenant)
        .execute(pool)
        .await?;
    let mut tx = pool.begin().await?;
    assert!(packages::lock(&mut tx, own).await?.is_none());
    assert!(packages::lock(&mut tx, foreign).await?.is_some());
    tx.rollback().await?;
    println!(
        "PASS package fence: scoped active package read, upgrade blocked by writer, unrelated tenant concurrent, refreshed version after commit and canonical content digest"
    );
    Ok(())
}
