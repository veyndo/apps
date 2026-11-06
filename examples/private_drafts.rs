//! Runnable private autosaves on the disposable test schema; caller owns identity and rollback, environment admission denies by default here.
use serde_json::json;
use sqlx::PgConnection;
use veyndo_app_storage::{
    Error,
    drafts::{self, AdmissionFuture, Draft, EnvironmentAdmission, Owner},
};
struct NoEnvironments;
impl EnvironmentAdmission for NoEnvironments {
    fn owned<'a>(&'a self, _: &'a mut PgConnection, _: &'a str, _: &'a str) -> AdmissionFuture<'a> {
        Box::pin(async { Ok(false) })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let tenant = format!(
        "draft-example-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    // Synthetic trusted-host fixture, not request-body identity or a production login adapter.
    let owner = Owner {
        tenant: &tenant,
        actor: "synthetic-editor",
    };
    let manifest = json!({"id":"example","version":"unfinished","coreApi":"v1","runtime":"native",
        "name":{"es-ES":"Mi borrador","de-DE":"Mein Entwurf","en-GB":"My draft","it-IT":""},"permissions":[]});
    let mut tx = pool.begin().await?;
    let draft = Draft {
        id: "example",
        manifest: &manifest,
        environment: None,
        revision: Some(0),
    };
    assert_eq!(
        drafts::save(&mut tx, owner, &draft, &NoEnvironments).await?,
        1
    );
    assert!(matches!(
        drafts::save(&mut tx, owner, &draft, &NoEnvironments).await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(drafts::list(&mut tx, owner).await?[0]["manifest"], manifest);
    assert!(
        drafts::list(
            &mut tx,
            Owner {
                actor: "another-editor",
                ..owner
            }
        )
        .await?
        .is_empty()
    );
    assert!(matches!(
        drafts::save(
            &mut tx,
            owner,
            &Draft {
                environment: Some("unadmitted"),
                revision: Some(1),
                ..draft
            },
            &NoEnvironments
        )
        .await,
        Err(Error::Invalid("Draft sandbox does not belong to this shop"))
    ));
    drafts::remove(&mut tx, owner, "example").await?;
    assert!(drafts::list(&mut tx, owner).await?.is_empty());
    tx.rollback().await?;
    println!(
        "PASS private drafts: strict contract, private scope, stale-save denial, required environment admission and caller rollback"
    );
    Ok(())
}
