//! Disposable schema conversion/recovery demonstration; explicit synthetic language policy and rollback, never production admission.
use serde_json::{Value, json};
use veyndo_app_storage::{
    RecordKey, Scope,
    installation::HookFuture,
    model::{Entity, manifest::Manifest},
    records,
    schema_upgrade::{self, Validation},
};
struct SyntheticPolicy;
impl<'db> Validation<'db> for SyntheticPolicy {
    type Error = &'static str;
    fn fields<'a>(
        &'a self,
        _: &'a mut sqlx::Transaction<'db, sqlx::Postgres>,
        _: &'a Entity,
        fields: &'a Value,
    ) -> HookFuture<'a, Self::Error>
    where
        'db: 'a,
    {
        Box::pin(async move {
            if fields["quantity"].as_i64().is_some_and(|v| v >= 0) {
                Ok(())
            } else {
                Err("Synthetic quantity must be a nonnegative integer")
            }
        })
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let old: Manifest = serde_json::from_value(
        json!({"id":"example","version":"1.0.0","coreApi":"v1","runtime":"declarative","name":{"en":"Synthetic example"},"permissions":[],"entities":[{"name":"entries","fields":[{"name":"quantity","kind":"string"}]}]}),
    )?;
    let mut value = json!(old);
    value["version"] = json!("1.1.0");
    value["entities"][0]["fields"][0]["kind"] = json!("integer");
    value["schemaMigrations"] = json!([{"fromVersion":"1.0.0","steps":[{"op":"convert","entity":"entries","field":"quantity"}]}]);
    let next = serde_json::from_value(value)?;
    let scope = Scope {
        tenant: "synthetic-schema-example",
        app: "example",
    };
    let key = RecordKey {
        scope,
        entity: "entries",
        id: "item",
    };
    let mut tx = pool.begin().await?;
    // A real service uses its current authority, complete compiler and shared installer fences.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,543)),pg_advisory_xact_lock(hashtextextended($2,7))").bind(scope.tenant).bind(format!("{}:{}",scope.tenant,scope.app)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO app_packages(tenant,id,manifest) VALUES($1,$2,$3)")
        .bind(scope.tenant)
        .bind(scope.app)
        .bind(json!(old))
        .execute(&mut *tx)
        .await?;
    records::define(
        &mut tx,
        scope,
        &old.version,
        "entries",
        &json!(old.entities[0]),
    )
    .await?;
    records::put(&mut tx, key, 1, &json!({"quantity":"12"})).await?;
    schema_upgrade::upgrade(&mut tx, scope, &old, &next, &SyntheticPolicy)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let row = records::get(&mut tx, key, false).await?.unwrap();
    assert_eq!(row["quantity"], 12);
    assert_eq!(row["revision"], 2);
    let backup: Value =
        sqlx::query_scalar("SELECT records FROM app_schema_history WHERE tenant=$1 AND app=$2")
            .bind(scope.tenant)
            .bind(scope.app)
            .fetch_one(&mut *tx)
            .await?;
    assert_eq!(backup["entries"][0]["quantity"], "12");
    tx.rollback().await?;
    println!(
        "PASS schema upgrade: exact conversion, bumped revision, retained original value, caller rollback; substitute actual host validation before production use"
    );
    Ok(())
}
