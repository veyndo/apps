//! Executable disposable-database settings round trip with ephemeral AEAD, secret-safe projection and full rollback.
#[path = "support/fixture_crypto.rs"]
mod fixture_crypto;
use fixture_crypto::FixtureCrypto;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};
use veyndo_app_storage::{
    Scope,
    model::settings,
    packages,
    secrets::{self, Write},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = sqlx::PgPool::connect(&std::env::var("APP_STORAGE_TEST_DATABASE_URL")?).await?;
    let fields: Vec<settings::Setting> = serde_json::from_value(json!([
        {"key":"api_token","kind":"secret","label":{"en":"Token"},"required":true},
        {"key":"caption","kind":"string","label":{"en":"Caption"}}
    ]))?;
    let values = json!({"api_token":"synthetic-demo-secret","caption":"Connected"});
    let encoded = settings::prepare(&fields, &json!({}), &values)
        .map_err(|_| "invalid synthetic settings")?;
    let manifest = json!({"id":"example","version":"1.0.0","settings":fields});
    let digest = packages::digest(&manifest);
    let tenant = format!(
        "settings-example-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    );
    let scope = Scope {
        tenant: &tenant,
        app: "example",
    };
    let mut key = [0u8; 32];
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| "fixture entropy")?;
    let crypto = FixtureCrypto(key);
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO app_packages(tenant,id,manifest,digest) VALUES($1,$2,$3,$4)")
        .bind(scope.tenant)
        .bind(scope.app)
        .bind(&manifest)
        .bind(&digest)
        .execute(&mut *tx)
        .await?;
    let revision = secrets::rotate(
        &mut tx,
        scope,
        &Write {
            kind: "settings",
            digest: &digest,
            revision: 0,
            actor: "fixture-owner",
            secret: &encoded,
        },
        &crypto,
    )
    .await
    .map_err(|_| "fixture rotation failed")?;
    assert_eq!(revision, 1);
    let plain = secrets::resolve(&mut tx, scope, "settings", &digest, &crypto)
        .await
        .map_err(|_| "fixture resolution failed")?
        .unwrap();
    let resolved: Value = serde_json::from_str(&plain)?;
    assert_eq!(resolved, values);
    assert_eq!(
        settings::visible(&fields, &resolved),
        json!({"caption":"Connected"})
    );
    assert_eq!(
        settings::configured(&fields, &resolved),
        vec!["api_token", "caption"]
    );
    assert!(
        !serde_json::to_string(&secrets::list(&mut tx, scope).await?)?
            .contains("synthetic-demo-secret")
    );
    tx.rollback().await?;
    println!(
        "PASS independent encrypted settings: model preparation, authenticated storage, internal resolution and secret-safe projection; fixture rolled back"
    );
    Ok(())
}
