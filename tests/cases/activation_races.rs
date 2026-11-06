//! Actual install/state/write fences, competing CAS and overflow; dependency admission cannot race deactivation in either direction.
use super::*;
use crate::installation::{Hooks, package as install_package};
use sqlx::{Postgres, Transaction};
use std::sync::Arc;
use tokio::sync::Barrier;
use veyndo_app_storage::{
    installation::{self, HookFuture, InstallError, InstallHooks},
    packages,
};
struct Install<'a> {
    tenant: &'a str,
    dependency: &'a str,
}
impl<'db> InstallHooks<'db> for Install<'_> {
    type Error = sqlx::Error;
    fn prepare<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { Hooks { fail: "" }.prepare(tx).await })
    }
    fn admit<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            let active: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM app_packages WHERE tenant=$1 AND id=$2 AND active)",
            )
            .bind(self.tenant)
            .bind(self.dependency)
            .fetch_one(&mut **tx)
            .await?;
            if !active {
                return Err(sqlx::Error::Protocol("required dependency inactive".into()));
            }
            Hooks { fail: "" }.admit(tx).await
        })
    }
    fn upgrade<'a>(
        &'a self,
        tx: &'a mut Transaction<'db, Postgres>,
        prior: &'a Value,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { Hooks { fail: "" }.upgrade(tx, prior).await })
    }
    fn attach<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { Hooks { fail: "" }.attach(tx).await })
    }
    fn finish<'a>(&'a self, tx: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move { Hooks { fail: "" }.finish(tx).await })
    }
}
async fn timeout(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL lock_timeout='150ms'")
        .execute(&mut **tx)
        .await?;
    Ok(())
}
fn blocked(error: &sqlx::Error) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("55P03")
    );
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Scope {
        tenant: "activation-race",
        app: "base",
    };
    package(pool, own, &json!({"id":"base","version":"1.0.0"})).await?;
    let barrier = Arc::new(Barrier::new(2));
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let pool = pool.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            set(
                &pool,
                Scope {
                    tenant: "activation-race",
                    app: "base",
                },
                false,
                Some(1),
                "",
            )
            .await
        }));
    }
    let mut successes = 0;
    let mut conflicts = 0;
    for task in tasks {
        match task.await? {
            Ok(_) => successes += 1,
            Err(ChangeError::Conflict("App revision changed")) => conflicts += 1,
            other => panic!("unexpected state result {other:?}"),
        }
    }
    assert_eq!((successes, conflicts), (1, 1));
    set(pool, own, true, Some(2), "").await.unwrap();
    // A normal active-package writer's shared fence prevents a concurrent state mutation.
    let mut writer = pool.begin().await?;
    assert!(packages::lock(&mut writer, own).await?.is_some());
    let mut changing = pool.begin().await?;
    timeout(&mut changing).await?;
    let Err(ChangeError::Database(error)) = activation::set(
        &mut changing,
        &Change {
            scope: own,
            active: false,
            revision: Some(3),
        },
        &Host {
            scope: own,
            fail: "",
        },
    )
    .await
    else {
        panic!("state escaped writer fence")
    };
    blocked(&error);
    changing.rollback().await?;
    writer.rollback().await?;
    // Hold a successful deactivation before commit. The actual installer must wait on the shared tenant fence.
    let mut held = pool.begin().await?;
    activation::set(
        &mut held,
        &Change {
            scope: own,
            active: false,
            revision: Some(3),
        },
        &Host {
            scope: own,
            fail: "",
        },
    )
    .await
    .unwrap();
    let manifest =
        json!({"id":"child","version":"1.0.0","distribution":{"dependencies":[{"app":"base"}]}});
    let host = Install {
        tenant: own.tenant,
        dependency: "base",
    };
    let mut install = pool.begin().await?;
    timeout(&mut install).await?;
    let Err(InstallError::Database(error)) = installation::install(
        &mut install,
        &install_package(own.tenant, &manifest, true),
        &host,
    )
    .await
    else {
        panic!("install escaped deactivation fence")
    };
    blocked(&error);
    install.rollback().await?;
    held.commit().await?;
    let mut install = pool.begin().await?;
    assert!(
        matches!(installation::install(&mut install,&install_package(own.tenant,&manifest,true),&host).await,Err(InstallError::Hook(sqlx::Error::Protocol(s))) if s.contains("dependency inactive"))
    );
    install.rollback().await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_packages WHERE tenant=$1 AND id='child'"
        )
        .bind(own.tenant)
        .fetch_one(pool)
        .await?,
        0
    );
    set(pool, own, true, Some(4), "").await.unwrap();
    // Reverse ordering: a successful install held before commit fences deactivation, then the new dependant blocks it.
    let mut install = pool.begin().await?;
    installation::install(
        &mut install,
        &install_package(own.tenant, &manifest, true),
        &host,
    )
    .await
    .unwrap();
    let mut changing = pool.begin().await?;
    timeout(&mut changing).await?;
    let Err(ChangeError::Database(error)) = activation::set(
        &mut changing,
        &Change {
            scope: own,
            active: false,
            revision: Some(5),
        },
        &Host {
            scope: own,
            fail: "",
        },
    )
    .await
    else {
        panic!("state escaped install fence")
    };
    blocked(&error);
    changing.rollback().await?;
    install.commit().await?;
    assert!(matches!(
        set(pool, own, false, Some(5), "").await,
        Err(ChangeError::Conflict(
            "App is required by another active app"
        ))
    ));
    assert_eq!(state(pool, own).await?, (true, 5));
    let exhausted = Scope {
        tenant: "activation-exhausted",
        app: "base",
    };
    package(pool, exhausted, &json!({"id":"base"})).await?;
    sqlx::query("UPDATE app_packages SET revision=$1 WHERE tenant=$2 AND id=$3")
        .bind(i64::MAX)
        .bind(exhausted.tenant)
        .bind(exhausted.app)
        .execute(pool)
        .await?;
    assert!(matches!(
        set(pool, exhausted, false, Some(i64::MAX), "").await,
        Err(ChangeError::Database(_))
    ));
    assert_eq!(state(pool, exhausted).await?, (true, i64::MAX));
    println!(
        "PASS state races: competing CAS, actual shared writer/install fences in both dependency orders and exhausted revision rollback"
    );
    Ok(())
}
