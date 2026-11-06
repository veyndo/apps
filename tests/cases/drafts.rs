//! Real private autosave contract: actor/tenant isolation, original wire/error semantics, rollback, restart and no installation effects.
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use veyndo_app_storage::{
    Error,
    drafts::{self, AdmissionFuture, Draft, EnvironmentAdmission, Owner},
};
#[path = "draft_races.rs"]
mod races;
pub(super) struct Host;
impl EnvironmentAdmission for Host {
    fn owned<'a>(
        &'a self,
        tx: &'a mut PgConnection,
        tenant: &'a str,
        environment: &'a str,
    ) -> AdmissionFuture<'a> {
        Box::pin(async move {
            sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM shop_environments WHERE live_tenant=$1 AND tenant=$2)",
            )
            .bind(tenant)
            .bind(environment)
            .fetch_one(tx)
            .await
        })
    }
}
pub(super) fn manifest(id: &str, text: &str) -> Value {
    json!({"id":id,"version":"work in progress","coreApi":"v1","runtime":"native",
        "name":{"es-ES":text,"de-DE":"Entwurf","fr-FR":"Brouillon","it-IT":"","pl-PL":"Szkic"},
        "permissions":[],"entities":[]})
}
pub(super) async fn save(
    pool: &PgPool,
    tenant: &str,
    actor: &str,
    id: &str,
    text: &str,
    revision: i64,
) -> Result<i64, Error> {
    let m = manifest(id, text);
    let mut tx = pool.begin().await?;
    let next = drafts::save(
        &mut tx,
        Owner { tenant, actor },
        &Draft {
            id,
            manifest: &m,
            environment: None,
            revision: Some(revision),
        },
        &Host,
    )
    .await?;
    tx.commit().await?;
    Ok(next)
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let own = Owner {
        tenant: "draft-own",
        actor: "alice",
    };
    let bob = Owner {
        actor: "bob",
        ..own
    };
    let foreign = Owner {
        tenant: "draft-foreign",
        ..own
    };
    let packages: i64 = sqlx::query_scalar("SELECT count(*) FROM app_packages")
        .fetch_one(pool)
        .await?;
    let events: i64 = sqlx::query_scalar("SELECT count(*) FROM outbox")
        .fetch_one(pool)
        .await?;
    for (owner, text) in [
        (own, "first"),
        (bob, "bob private"),
        (foreign, "foreign private"),
    ] {
        assert_eq!(
            save(pool, owner.tenant, owner.actor, "example", text, 0).await?,
            1
        );
    }
    let mut tx = pool.begin().await?;
    let initial = drafts::list(&mut tx, own).await?;
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0]["manifest"], manifest("example", "first"));
    assert_eq!(initial[0]["revision"], 1);
    assert!(initial[0]["updatedAt"].is_string());
    assert_eq!(initial[0]["environment"], Value::Null);
    drafts::remove(&mut tx, bob, "example").await?;
    assert!(drafts::list(&mut tx, bob).await?.is_empty());
    assert_eq!(drafts::list(&mut tx, own).await?, initial);
    tx.rollback().await?;
    assert!(matches!(
        save(pool, own.tenant, own.actor, "example", "stale", 0).await,
        Err(Error::Conflict(_))
    ));
    assert_eq!(
        save(pool, own.tenant, own.actor, "example", "edited", 1).await?,
        2
    );

    sqlx::query("INSERT INTO shop_environments(tenant,live_tenant) VALUES('draft-stage','draft-own'),('draft-foreign-stage','draft-foreign')").execute(pool).await?;
    let m = manifest("example", "private stage");
    let mut tx = pool.begin().await?;
    let next = drafts::save(
        &mut tx,
        own,
        &Draft {
            id: "example",
            manifest: &m,
            environment: Some("draft-stage"),
            revision: Some(2),
        },
        &Host,
    )
    .await?;
    assert_eq!(next, 3);
    assert_eq!(
        drafts::list(&mut tx, own).await?[0]["environment"],
        "draft-stage"
    );
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    for environment in ["draft-foreign-stage", "missing-stage", own.tenant] {
        assert!(matches!(
            drafts::save(
                &mut tx,
                own,
                &Draft {
                    id: "example",
                    manifest: &m,
                    environment: Some(environment),
                    revision: Some(2)
                },
                &Host
            )
            .await,
            Err(Error::Invalid("Draft sandbox does not belong to this shop"))
        ));
    }
    assert_eq!(drafts::list(&mut tx, own).await?[0]["revision"], 2);
    tx.rollback().await?;
    // The required host check failing must not replace the durable previous draft.
    struct BrokenHost;
    impl EnvironmentAdmission for BrokenHost {
        fn owned<'a>(
            &'a self,
            _: &'a mut PgConnection,
            _: &'a str,
            _: &'a str,
        ) -> AdmissionFuture<'a> {
            Box::pin(async { Err(sqlx::Error::Protocol("fixture unavailable".into())) })
        }
    }
    let mut tx = pool.begin().await?;
    assert!(matches!(
        drafts::save(
            &mut tx,
            own,
            &Draft {
                id: "example",
                manifest: &m,
                environment: Some("draft-stage"),
                revision: Some(2)
            },
            &BrokenHost
        )
        .await,
        Err(Error::Database(_))
    ));
    tx.rollback().await?;
    invalid_inputs(pool, own).await?;
    // Caller rollback also restores a scoped deletion, not only an autosave.
    let mut tx = pool.begin().await?;
    drafts::remove(&mut tx, own, "example").await?;
    tx.rollback().await?;
    let reopened = PgPool::connect_with(pool.connect_options().as_ref().clone()).await?;
    let mut tx = reopened.begin().await?;
    assert_eq!(
        drafts::list(&mut tx, own).await?[0]["manifest"],
        manifest("example", "edited")
    );
    assert_eq!(
        drafts::list(&mut tx, foreign).await?[0]["manifest"],
        manifest("example", "foreign private")
    );
    assert_eq!(
        drafts::list(&mut tx, bob).await?[0]["manifest"],
        manifest("example", "bob private")
    );
    tx.rollback().await?;
    reopened.close().await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app_packages")
            .fetch_one(pool)
            .await?,
        packages
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox")
            .fetch_one(pool)
            .await?,
        events
    );
    println!(
        "PASS private drafts: actor/tenant and environment scope, exact wire/locale preservation, 64 KiB boundary, rollback, reconnect and no installed effects"
    );
    races::run(pool).await?;
    Ok(())
}
async fn invalid_inputs(pool: &PgPool, owner: Owner<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let m = manifest("example", "unchanged");
    let mut unknown = m.clone();
    unknown["unregistered"] = json!(true);
    let mut tx = pool.begin().await?;
    for (id, value, revision, message) in [
        (
            "bad-id",
            &m,
            Some(0),
            "Draft identifier or 64 KiB budget invalid",
        ),
        (
            "example",
            &unknown,
            Some(0),
            "Draft must use the shared app contract",
        ),
        ("another", &m, Some(0), "Draft identity mismatch"),
        ("example", &m, None, "Draft revision required"),
        ("example", &m, Some(-1), "Draft revision required"),
    ] {
        assert!(
            matches!(drafts::save(&mut tx,owner,&Draft {id,manifest:value,environment:None,revision},&Host).await,Err(Error::Invalid(s)) if s==message)
        );
    }
    let mut boundary = manifest("boundary", "");
    let overhead = boundary.to_string().len();
    boundary["name"]["es-ES"] = json!("x".repeat(65536 - overhead));
    assert_eq!(boundary.to_string().len(), 65536);
    assert_eq!(
        drafts::save(
            &mut tx,
            owner,
            &Draft {
                id: "boundary",
                manifest: &boundary,
                environment: Some(""),
                revision: Some(0)
            },
            &Host
        )
        .await?,
        1
    );
    boundary["name"]["es-ES"] = json!("é".repeat(32769));
    assert!(matches!(
        drafts::save(
            &mut tx,
            owner,
            &Draft {
                id: "boundary",
                manifest: &boundary,
                environment: None,
                revision: Some(1)
            },
            &Host
        )
        .await,
        Err(Error::Invalid("Draft identifier or 64 KiB budget invalid"))
    ));
    tx.rollback().await?;
    Ok(())
}
