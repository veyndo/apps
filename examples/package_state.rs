//! Runnable mandatory state-admission boundary on the disposable schema; no implicit host grants or transaction commit.
use sqlx::{Postgres, Transaction};
use veyndo_app_storage::{
    Scope,
    activation::{self, Change, ChangeError, StateHooks},
    installation::HookFuture,
};
struct NoAuthority;
impl<'db> StateHooks<'db> for NoAuthority {
    type Error = &'static str;
    fn prepare<'a>(&'a self, _: &'a mut Transaction<'db, Postgres>) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async {
            Err(
                "Supply the actual current host identity/configuration admission before changing state",
            )
        })
    }
    fn activating<'a>(
        &'a self,
        _: &'a mut Transaction<'db, Postgres>,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async {
            Err("Supply actual package/actor/consent/trust/dependency/commerce/lifecycle admission")
        })
    }
    fn deactivating<'a>(
        &'a self,
        _: &'a mut Transaction<'db, Postgres>,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async { Err("Supply actual connected-frontend admission") })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let scope = Scope {
        tenant: "synthetic-state-example",
        app: "example",
    };
    let mut tx = pool.begin().await?;
    for active in [false, true] {
        assert!(matches!(
            activation::set(
                &mut tx,
                &Change {
                    scope,
                    active,
                    revision: Some(1)
                },
                &NoAuthority
            )
            .await,
            Err(ChangeError::Host(_))
        ));
    }
    tx.rollback().await?;
    println!(
        "PASS package state: both paths reject missing actual host authority; no implicit activation, deactivation or commit"
    );
    Ok(())
}
