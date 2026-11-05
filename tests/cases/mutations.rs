//! Actual PostgreSQL record mutation concurrency, hook rollback, scoped identity and immutable replay cases.
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use veyndo_app_storage::{
    RecordKey, Scope,
    mutations::{self, Mutation, WriteError, WriteHooks},
    records,
};
struct Audit {
    fail: &'static str,
}
impl WriteHooks for Audit {
    type Error = sqlx::Error;
    async fn before(
        &self,
        tx: &mut PgConnection,
        _old: Option<&Value>,
        _fields: &Value,
        revision: i64,
    ) -> Result<(), Self::Error> {
        sqlx::query("INSERT INTO mutation_audit(revision,phase) VALUES($1,'before')")
            .bind(revision)
            .execute(tx)
            .await?;
        if self.fail == "before" {
            return Err(sqlx::Error::Protocol("host workflow denied".into()));
        }
        Ok(())
    }
    async fn after(
        &self,
        tx: &mut PgConnection,
        _fields: &Value,
        revision: i64,
    ) -> Result<(), Self::Error> {
        sqlx::query("INSERT INTO mutation_audit(revision,phase) VALUES($1,'after')")
            .bind(revision)
            .execute(tx)
            .await?;
        if self.fail == "after" {
            return Err(sqlx::Error::Protocol("host audit unavailable".into()));
        }
        Ok(())
    }
}
const NAMES: &[&str] = &["title", "active"];
fn change<'a>(
    scope: Scope<'a>,
    id: &'a str,
    expected: i64,
    fields: &'a Value,
    replay: bool,
) -> Mutation<'a> {
    Mutation {
        key: RecordKey {
            scope,
            entity: "items",
            id,
        },
        expected_revision: expected,
        fields,
        declared_fields: NAMES,
        replay,
    }
}
async fn counts(pool: &PgPool) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as("SELECT (SELECT count(*) FROM mutation_audit),(SELECT count(*) FROM outbox WHERE kind='app.record.changed')").fetch_one(pool).await
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query("CREATE TABLE mutation_audit(revision bigint NOT NULL,phase text NOT NULL)")
        .execute(pool)
        .await?;
    sqlx::query("ALTER TABLE outbox ADD CONSTRAINT mutation_event_failure CHECK(data->>'id' IS DISTINCT FROM 'reject-event')").execute(pool).await?;
    let own = Scope {
        tenant: "mutation-own",
        app: "catalog",
    };
    let other = Scope {
        tenant: "mutation-other",
        app: "catalog",
    };
    let fields = json!({"title":{"fr":"Lampe","it":null},"active":true});
    let hooks = Audit { fail: "" };
    for scope in [own, other] {
        let mut tx = pool.begin().await?;
        records::define(&mut tx, scope, "1", "items", &json!({})).await?;
        let result = mutations::write(&mut tx, &change(scope, "same", 0, &fields, false), &hooks)
            .await
            .unwrap();
        assert_eq!(result, json!({"id":"same","revision":1,"fields":fields}));
        tx.commit().await?;
    }
    let before = counts(pool).await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        mutations::write(&mut tx, &change(own, "same", 0, &fields, false), &hooks).await,
        Err(WriteError::Revision(1))
    ));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    let result = mutations::write(&mut tx, &change(own, "same", 0, &fields, true), &hooks)
        .await
        .unwrap();
    assert_eq!(result["replayed"], true);
    tx.commit().await?;
    let changed = json!({"title":{"fr":"Différent","it":null},"active":true});
    let mut tx = pool.begin().await?;
    assert!(matches!(
        mutations::write(&mut tx, &change(own, "same", 1, &changed, true), &hooks).await,
        Err(WriteError::Conflict(_))
    ));
    tx.rollback().await?;
    assert_eq!(counts(pool).await?, before);
    for fail in ["before", "after"] {
        let mut tx = pool.begin().await?;
        assert!(matches!(
            mutations::write(
                &mut tx,
                &change(own, "same", 1, &changed, false),
                &Audit { fail }
            )
            .await,
            Err(WriteError::Hook(_))
        ));
        tx.rollback().await?;
        let mut tx = pool.begin().await?;
        assert_eq!(
            records::get(&mut tx, change(own, "same", 0, &fields, false).key, false)
                .await?
                .unwrap()["revision"],
            1
        );
        tx.rollback().await?;
        assert_eq!(counts(pool).await?, before);
    }
    let mut tx = pool.begin().await?;
    assert!(matches!(
        mutations::write(
            &mut tx,
            &change(own, "reject-event", 0, &fields, false),
            &hooks
        )
        .await,
        Err(WriteError::Database(_))
    ));
    tx.rollback().await?;
    assert_eq!(counts(pool).await?, before);
    let write = |pool: PgPool, title: &'static str| async move {
        let mut tx = pool.begin().await.unwrap();
        let fields = json!({"title":{"fr":title,"it":null},"active":true});
        match mutations::write(
            &mut tx,
            &change(own, "same", 1, &fields, false),
            &Audit { fail: "" },
        )
        .await
        {
            Ok(_) => {
                tx.commit().await.unwrap();
                true
            }
            Err(WriteError::Revision(2)) => {
                tx.rollback().await.unwrap();
                false
            }
            other => panic!("unexpected concurrent result {other:?}"),
        }
    };
    let (left, right) = tokio::join!(write(pool.clone(), "One"), write(pool.clone(), "Two"));
    assert_ne!(left, right);
    assert_eq!(counts(pool).await?, (before.0 + 2, before.1 + 1));
    let mut tx = pool.begin().await?;
    assert_eq!(
        records::get(&mut tx, change(other, "same", 0, &fields, false).key, false)
            .await?
            .unwrap()["title"],
        fields["title"]
    );
    records::put(
        &mut tx,
        change(own, "max", 0, &fields, false).key,
        i64::MAX,
        &fields,
    )
    .await?;
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        mutations::write(
            &mut tx,
            &change(own, "max", i64::MAX, &fields, false),
            &hooks
        )
        .await,
        Err(WriteError::Conflict(_))
    ));
    tx.rollback().await?;
    println!(
        "PASS record mutations: CAS race, exact replay, changed import denial, hooks/outbox rollback, foreign scope, translations and revision overflow"
    );
    Ok(())
}
