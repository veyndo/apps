//! Real SQL row/byte budgets, no-op paths and retained recovery isolation; every rejected operation rolls back.
use super::{Policy, manifests, setup, state};
use serde_json::json;
use sqlx::PgPool;
use veyndo_app_storage::{
    Scope,
    schema_upgrade::{self, UpgradeError},
};
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let (old, next) = manifests();
    for (tenant, count) in [("schema-2000", 2000), ("schema-2001", 2001)] {
        setup(pool, tenant, &old, "7", 1).await?;
        sqlx::query("INSERT INTO app_records(tenant,app,entity,id,revision,version,fields) SELECT tenant,app,entity,'row-'||n,revision,version,fields FROM app_records CROSS JOIN generate_series(2,$2::integer) n WHERE tenant=$1 AND id='same'").bind(tenant).bind(count).execute(pool).await?;
        let before = state(pool, tenant, &old.id).await?;
        let mut tx = pool.begin().await?;
        let result = schema_upgrade::upgrade(
            &mut tx,
            Scope {
                tenant,
                app: &old.id,
            },
            &old,
            &next,
            &Policy {
                tenant,
                deny: false,
            },
        )
        .await;
        if count == 2000 {
            assert!(result.is_ok(), "{result:?}");
            let revisions: i64=sqlx::query_scalar("SELECT count(*) FROM app_records WHERE tenant=$1 AND revision=2 AND fields->>'count'='7'").bind(tenant).fetch_one(&mut *tx).await?;
            assert_eq!(revisions, 2000);
        } else {
            assert!(
                matches!(result,Err(UpgradeError::Conflict(message)) if message.contains("2000 records"))
            );
        }
        tx.rollback().await?;
        assert_eq!(state(pool, tenant, &old.id).await?, before);
    }
    setup(pool, "schema-large", &old, "7", 1).await?;
    // Old declared strings from historical storage can exceed current field limits.
    sqlx::query("UPDATE app_records SET fields=jsonb_set(fields,'{obsolete}',to_jsonb(repeat('x',4000000))) WHERE tenant='schema-large'").execute(pool).await?;
    let before = state(pool, "schema-large", &old.id).await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        schema_upgrade::upgrade(
            &mut tx,
            Scope {
                tenant: "schema-large",
                app: &old.id
            },
            &old,
            &next,
            &Policy {
                tenant: "schema-large",
                deny: false
            }
        )
        .await,
        Err(UpgradeError::Conflict(
            "Schema recovery snapshot exceeds 4 MB"
        ))
    ));
    tx.rollback().await?;
    assert_eq!(state(pool, "schema-large", &old.id).await?, before);
    for tenant in ["schema-retained", "schema-retained-foreign"] {
        setup(pool, tenant, &old, "7", 1).await?;
        sqlx::query("INSERT INTO app_schema_history(tenant,app,to_version,from_version,old_manifest,records) SELECT $1,$2,'prior-'||n,'prior','{}',jsonb_build_object('old',repeat('x',4000000)) FROM generate_series(1,4) n").bind(tenant).bind(&old.id).execute(pool).await?;
    }
    let before = state(pool, "schema-retained", &old.id).await?;
    let mut tx = pool.begin().await?;
    assert!(
        matches!(schema_upgrade::upgrade(&mut tx,Scope{tenant:"schema-retained",app:&old.id},&old,&next,&Policy{tenant:"schema-retained",deny:false}).await,Err(UpgradeError::Conflict(message)) if message.contains("quota reached"))
    );
    tx.rollback().await?;
    assert_eq!(state(pool, "schema-retained", &old.id).await?, before);
    setup(pool, "schema-retained-other-app", &old, "7", 1).await?;
    let mut other = old.clone();
    other.id = "another_app".into();
    setup(pool, "schema-retained-other-app", &other, "7", 1).await?;
    sqlx::query("INSERT INTO app_schema_history(tenant,app,to_version,from_version,old_manifest,records) SELECT 'schema-retained-other-app','another_app',to_version,from_version,old_manifest,records FROM app_schema_history WHERE tenant='schema-retained'").execute(pool).await?;
    let mut tx = pool.begin().await?;
    assert!(
        schema_upgrade::upgrade(
            &mut tx,
            Scope {
                tenant: "schema-retained-other-app",
                app: &old.id
            },
            &old,
            &next,
            &Policy {
                tenant: "schema-retained-other-app",
                deny: false
            }
        )
        .await
        .is_ok()
    );
    tx.rollback().await?;
    // Identical versions and compatible additive versions never capture or emit a migration event.
    setup(pool, "schema-noop", &old, "7", 1).await?;
    let before = state(pool, "schema-noop", &old.id).await?;
    let mut additive = old.clone();
    additive.version = "1.0.1".into();
    let mut tx = pool.begin().await?;
    for m in [&old, &additive] {
        assert!(
            schema_upgrade::upgrade(
                &mut tx,
                Scope {
                    tenant: "schema-noop",
                    app: &old.id
                },
                &old,
                m,
                &Policy {
                    tenant: "schema-noop",
                    deny: true
                }
            )
            .await
            .is_ok()
        );
    }
    tx.commit().await?;
    assert_eq!(state(pool, "schema-noop", &old.id).await?, before);
    // A failed unique snapshot insert cannot leave converted fields or model definitions behind.
    sqlx::query("INSERT INTO app_schema_history(tenant,app,to_version,from_version,old_manifest,records) VALUES('schema-noop',$1,'1.1.0','1.0.0',$2,'{}')").bind(&old.id).bind(json!(old)).execute(pool).await?;
    let before = state(pool, "schema-noop", &old.id).await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        schema_upgrade::upgrade(
            &mut tx,
            Scope {
                tenant: "schema-noop",
                app: &old.id
            },
            &old,
            &next,
            &Policy {
                tenant: "schema-noop",
                deny: false
            }
        )
        .await,
        Err(UpgradeError::Database(_))
    ));
    tx.rollback().await?;
    assert_eq!(state(pool, "schema-noop", &old.id).await?, before);
    Ok(())
}
