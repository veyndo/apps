//! Real scoped package-state/CAS and data retention, required current host facts, branch error order and rollback.
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use veyndo_app_storage::{
    Scope,
    activation::{self, Change, ChangeError},
    records,
};
#[path = "activation_host.rs"]
mod host;
use host::Host;
#[path = "activation_races.rs"]
mod races;
pub(super) async fn package(
    pool: &PgPool,
    scope: Scope<'_>,
    manifest: &Value,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO activation_hosts(tenant,can_manage) VALUES($1,true) ON CONFLICT DO NOTHING",
    )
    .bind(scope.tenant)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO app_packages(tenant,id,manifest) VALUES($1,$2,$3)")
        .bind(scope.tenant)
        .bind(scope.app)
        .bind(manifest)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO activation_facts(tenant,app) VALUES($1,$2)")
        .bind(scope.tenant)
        .bind(scope.app)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
pub(super) async fn state(pool: &PgPool, scope: Scope<'_>) -> Result<(bool, i64), sqlx::Error> {
    sqlx::query_as("SELECT active,revision FROM app_packages WHERE tenant=$1 AND id=$2")
        .bind(scope.tenant)
        .bind(scope.app)
        .fetch_one(pool)
        .await
}
pub(super) async fn set(
    pool: &PgPool,
    scope: Scope<'_>,
    active: bool,
    revision: Option<i64>,
    fail: &'static str,
) -> Result<Value, ChangeError<String>> {
    let mut tx = pool.begin().await?;
    let result = activation::set(
        &mut tx,
        &Change {
            scope,
            active,
            revision,
        },
        &Host { scope, fail },
    )
    .await?;
    tx.commit().await?;
    Ok(result)
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::raw_sql("CREATE TABLE activation_hosts(tenant text PRIMARY KEY,can_manage boolean NOT NULL); CREATE TABLE activation_facts(tenant text NOT NULL,app text NOT NULL,consent_ok boolean NOT NULL DEFAULT true,trust_ok boolean NOT NULL DEFAULT true,commerce_ok boolean NOT NULL DEFAULT true,lifecycle_ok boolean NOT NULL DEFAULT true,PRIMARY KEY(tenant,app),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id)); CREATE TABLE activation_frontends(tenant text NOT NULL,app text NOT NULL,PRIMARY KEY(tenant,app),FOREIGN KEY(tenant,app) REFERENCES app_packages(tenant,id)); CREATE TABLE activation_audit(id bigserial PRIMARY KEY,tenant text NOT NULL,app text NOT NULL,phase text NOT NULL);").execute(pool).await?;
    let own = Scope {
        tenant: "activation-own",
        app: "example",
    };
    let foreign = Scope {
        tenant: "activation-foreign",
        ..own
    };
    let manifest = json!({"id":"example","version":"1.0.0"});
    for scope in [own, foreign] {
        package(pool, scope, &manifest).await?;
    }
    let mut tx = pool.begin().await?;
    records::define(&mut tx, own, "1.0.0", "notes", &json!({})).await?;
    records::put(
        &mut tx,
        veyndo_app_storage::RecordKey {
            scope: own,
            entity: "notes",
            id: "one",
        },
        1,
        &json!({"title":"retained"}),
    )
    .await?;
    sqlx::query("INSERT INTO app_versions(tenant,app,version,digest,manifest) VALUES($1,$2,'1.0.0','original',$3)").bind(own.tenant).bind(own.app).bind(&manifest).execute(&mut *tx).await?;
    tx.commit().await?;
    assert_eq!(
        set(pool, own, false, Some(1), "").await.unwrap(),
        json!({"active":false,"dataRetained":true})
    );
    assert_eq!(state(pool, own).await?, (false, 2));
    assert_eq!(state(pool, foreign).await?, (true, 1));
    assert!(matches!(
        set(pool, own, true, Some(1), "").await,
        Err(ChangeError::Conflict("App revision changed"))
    ));
    for fact in ["consent_ok", "trust_ok", "commerce_ok", "lifecycle_ok"] {
        fact_value(pool, own, fact, false).await?;
        assert!(
            matches!(set(pool,own,true,Some(2),"").await,Err(ChangeError::Host(s)) if s.contains(fact))
        );
        assert_eq!(state(pool, own).await?, (false, 2));
        fact_value(pool, own, fact, true).await?;
    }
    assert!(matches!(
        set(pool, own, true, None, "").await,
        Err(ChangeError::Invalid("revision required"))
    ));
    for (active, phase) in [
        (true, "prepare"),
        (true, "activating"),
        (false, "deactivating"),
    ] {
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM activation_audit")
            .fetch_one(pool)
            .await?;
        assert!(matches!(
            set(pool, own, active, Some(2), phase).await,
            Err(ChangeError::Host(_))
        ));
        assert_eq!(state(pool, own).await?, (false, 2));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM activation_audit")
                .fetch_one(pool)
                .await?,
            before
        );
    }
    set(pool, own, true, Some(2), "").await.unwrap();
    assert_eq!(state(pool, own).await?, (true, 3));
    // Same-state updates preserve the original non-idempotent revision semantics.
    set(pool, own, true, Some(3), "").await.unwrap();
    assert_eq!(state(pool, own).await?, (true, 4));
    sqlx::query("INSERT INTO activation_frontends(tenant,app) VALUES($1,$2)")
        .bind(own.tenant)
        .bind(own.app)
        .execute(pool)
        .await?;
    assert!(
        matches!(set(pool,own,false,None,"").await,Err(ChangeError::Host(s)) if s=="connected frontend")
    );
    sqlx::query("DELETE FROM activation_frontends WHERE tenant=$1")
        .bind(own.tenant)
        .execute(pool)
        .await?;
    let child = Scope {
        app: "child",
        ..own
    };
    package(
        pool,
        child,
        &json!({"id":"child","distribution":{"dependencies":[{"app":"example"}]}}),
    )
    .await?;
    assert!(matches!(
        set(pool, own, false, None, "").await,
        Err(ChangeError::Conflict(
            "App is required by another active app"
        ))
    ));
    set(pool, child, false, Some(1), "").await.unwrap();
    set(pool, own, false, Some(4), "").await.unwrap();
    assert_eq!(state(pool, own).await?, (false, 5));
    let mut tx = pool.begin().await?;
    assert_eq!(
        records::rows(&mut tx, own, "notes", 10).await?[0]["title"],
        "retained"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT digest FROM app_versions WHERE tenant=$1 AND app=$2"
        )
        .bind(own.tenant)
        .bind(own.app)
        .fetch_one(&mut *tx)
        .await?,
        "original"
    );
    tx.rollback().await?;
    let absent = Scope {
        app: "absent",
        ..own
    };
    assert!(
        matches!(set(pool,absent,true,None,"").await,Err(ChangeError::Host(s)) if s=="package unavailable")
    );
    assert!(matches!(
        set(pool, absent, false, Some(1), "").await,
        Err(ChangeError::Conflict("App revision changed"))
    ));
    sqlx::query("UPDATE activation_hosts SET can_manage=false WHERE tenant=$1")
        .bind(own.tenant)
        .execute(pool)
        .await?;
    assert!(
        matches!(set(pool,own,true,Some(5),"").await,Err(ChangeError::Host(s)) if s=="current management denied")
    );
    sqlx::query("UPDATE activation_hosts SET can_manage=true WHERE tenant=$1")
        .bind(own.tenant)
        .execute(pool)
        .await?;
    let reopened = PgPool::connect_with(pool.connect_options().as_ref().clone()).await?;
    assert_eq!(state(&reopened, own).await?, (false, 5));
    reopened.close().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM outbox WHERE tenant LIKE 'activation-%'"
        )
        .fetch_one(pool)
        .await?,
        0
    );
    races::run(pool).await?;
    println!(
        "PASS package state: current host facts, original errors/order and CAS, dependencies/frontends, retained data/versions, rollback/reconnect and no new events"
    );
    Ok(())
}
async fn fact_value(
    pool: &PgPool,
    scope: Scope<'_>,
    name: &str,
    value: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE activation_facts SET consent_ok=CASE WHEN $3='consent_ok' THEN $4 ELSE consent_ok END,trust_ok=CASE WHEN $3='trust_ok' THEN $4 ELSE trust_ok END,commerce_ok=CASE WHEN $3='commerce_ok' THEN $4 ELSE commerce_ok END,lifecycle_ok=CASE WHEN $3='lifecycle_ok' THEN $4 ELSE lifecycle_ok END WHERE tenant=$1 AND app=$2")
        .bind(scope.tenant).bind(scope.app).bind(name).bind(value).execute(pool).await?;
    Ok(())
}
