//! Independent encrypted storage: actual test AEAD binding, digest/CAS races, atomic events and webhook-share fencing.
use sqlx::PgPool;
use veyndo_app_storage::{
    Scope,
    secrets::{self, SecretError, Write},
};
#[path = "../../examples/support/fixture_crypto.rs"]
mod fixture_crypto;
use fixture_crypto::FixtureCrypto;
struct Unavailable;
impl secrets::Crypto for Unavailable {
    type Error = &'static str;
    fn seal(&self, _: &str, _: &str) -> Result<String, Self::Error> {
        Err("key unavailable")
    }
    fn open(&self, _: &str, _: &str) -> Result<String, Self::Error> {
        Err("key unavailable")
    }
}
fn write(revision: i64) -> Write<'static> {
    Write {
        kind: "settings",
        digest: "reviewed",
        revision,
        actor: "fixture-owner",
        secret: "synthetic-private-setting",
    }
}
async fn seed(pool: &PgPool, scope: Scope<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO app_packages(tenant,id,manifest,digest) VALUES($1,$2,'{}','reviewed')",
    )
    .bind(scope.tenant)
    .bind(scope.app)
    .execute(pool)
    .await?;
    Ok(())
}
pub async fn run(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    let crypto = FixtureCrypto([71; 32]);
    let own = Scope {
        tenant: "secret-own",
        app: "example",
    };
    let foreign = Scope {
        tenant: "secret-foreign",
        app: "example",
    };
    let other = Scope {
        tenant: "secret-own",
        app: "other",
    };
    for scope in [own, foreign, other] {
        seed(pool, scope).await?;
    }
    let mut tx = pool.begin().await?;
    assert!(secrets::metadata(&mut tx, own, "settings").await?.is_none());
    assert!(
        secrets::resolve(&mut tx, own, "settings", "reviewed", &crypto)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        secrets::rotate(&mut tx, own, &write(0), &crypto)
            .await
            .unwrap(),
        1
    );
    tx.commit().await?;
    let mut tx = pool.begin().await?;
    let metadata = secrets::metadata(&mut tx, own, "settings").await?.unwrap();
    assert_eq!(
        (metadata.revision, metadata.digest.as_str()),
        (1, "reviewed")
    );
    let list = secrets::list(&mut tx, own).await?;
    assert_eq!(list.len(), 1);
    assert!(!list[0].to_string().contains("synthetic-private-setting"));
    assert!(list[0].get("ciphertext").is_none());
    assert!(list[0].get("secret").is_none());
    assert_eq!(
        secrets::resolve(&mut tx, own, "settings", "reviewed", &crypto)
            .await
            .unwrap()
            .as_deref(),
        Some("synthetic-private-setting")
    );
    for scope in [foreign, other] {
        assert!(
            secrets::resolve(&mut tx, scope, "settings", "reviewed", &crypto)
                .await
                .unwrap()
                .is_none()
        );
    }
    assert!(matches!(
        secrets::resolve(&mut tx, own, "settings", "changed", &crypto).await,
        Err(SecretError::Conflict(_))
    ));
    assert!(matches!(
        secrets::resolve(
            &mut tx,
            own,
            "settings",
            "reviewed",
            &FixtureCrypto([72; 32])
        )
        .await,
        Err(SecretError::Crypto(_))
    ));
    assert!(matches!(
        secrets::rotate(&mut tx, own, &write(0), &crypto).await,
        Err(SecretError::Conflict(
            "Secret was rotated elsewhere; reload its revision"
        ))
    ));
    for revision in [-1, i64::MAX] {
        assert!(matches!(
            secrets::rotate(&mut tx, own, &write(revision), &crypto).await,
            Err(SecretError::Invalid(_))
        ));
    }
    let mut changed = write(1);
    changed.digest = "changed";
    assert!(matches!(
        secrets::rotate(&mut tx, own, &changed, &crypto).await,
        Err(SecretError::Conflict(
            "App package changed before secret rotation"
        ))
    ));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        secrets::rotate(&mut tx, own, &write(1), &Unavailable).await,
        Err(SecretError::Crypto("key unavailable"))
    ));
    assert_eq!(
        secrets::metadata(&mut tx, own, "settings")
            .await?
            .unwrap()
            .revision,
        1
    );
    tx.rollback().await?;
    // Replaying an authenticated envelope in any other bound dimension must fail.
    for dimension in ["tenant", "app", "kind", "revision"] {
        let mut tx = pool.begin().await?;
        let destination = match dimension {
            "tenant" => foreign,
            "app" => other,
            _ => own,
        };
        sqlx::query("INSERT INTO app_service_secrets(tenant,app,kind,ciphertext,package_digest,revision,actor) SELECT $1,$2,$3,ciphertext,package_digest,$4,actor FROM app_service_secrets WHERE tenant=$5 AND app=$6 AND kind='settings' ON CONFLICT(tenant,app,kind) DO UPDATE SET revision=EXCLUDED.revision")
            .bind(destination.tenant).bind(destination.app).bind(if dimension=="kind" {"service"} else {"settings"}).bind(if dimension=="revision" {2i64} else {1}).bind(own.tenant).bind(own.app).execute(&mut *tx).await?;
        assert!(matches!(
            secrets::resolve(
                &mut tx,
                destination,
                if dimension == "kind" {
                    "service"
                } else {
                    "settings"
                },
                "reviewed",
                &crypto
            )
            .await,
            Err(SecretError::Crypto(_))
        ));
        tx.rollback().await?;
    }
    // Failure of the outbox rejects the encrypted write in the same transaction.
    let rejected = Scope {
        tenant: "secret-reject",
        app: "example",
    };
    seed(pool, rejected).await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        secrets::rotate(&mut tx, rejected, &write(0), &crypto).await,
        Err(SecretError::Database(_))
    ));
    tx.rollback().await?;
    let mut tx = pool.begin().await?;
    assert!(
        secrets::metadata(&mut tx, rejected, "settings")
            .await?
            .is_none()
    );
    tx.commit().await?;
    // Two controllers cannot both replace the same revision.
    let mut tasks = vec![];
    for secret in ["synthetic-left", "synthetic-right"] {
        let pool = pool.clone();
        tasks.push(tokio::spawn(async move {
            let mut tx = pool.begin().await.unwrap();
            let mut value = write(1);
            value.secret = secret;
            let result = secrets::rotate(&mut tx, own, &value, &FixtureCrypto([71; 32])).await;
            if result.is_ok() {
                tx.commit().await.unwrap();
                true
            } else {
                assert!(matches!(result, Err(SecretError::Conflict(_))));
                tx.rollback().await.unwrap();
                false
            }
        }));
    }
    assert_ne!(tasks.remove(0).await?, tasks.remove(0).await?);
    let mut tx = pool.begin().await?;
    let old = secrets::resolve_locked(&mut tx, own, "settings", "reviewed", &crypto)
        .await
        .unwrap();
    let (started, ready) = tokio::sync::oneshot::channel();
    let concurrent = pool.clone();
    let mut task = tokio::spawn(async move {
        let mut tx = concurrent.begin().await.unwrap();
        started.send(()).unwrap();
        let result = secrets::rotate(&mut tx, own, &write(2), &FixtureCrypto([71; 32]))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    });
    ready.await?;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(150), &mut task)
            .await
            .is_err()
    );
    assert_eq!(
        secrets::resolve_locked(&mut tx, own, "settings", "reviewed", &crypto)
            .await
            .unwrap(),
        old
    );
    tx.commit().await?;
    assert_eq!(task.await?, 3);
    // A newer installed package never inherits an older envelope silently.
    sqlx::query("UPDATE app_packages SET digest='new-package' WHERE tenant=$1 AND id=$2")
        .bind(own.tenant)
        .bind(own.app)
        .execute(pool)
        .await?;
    let mut tx = pool.begin().await?;
    assert!(matches!(
        secrets::resolve(&mut tx, own, "settings", "new-package", &crypto).await,
        Err(SecretError::Conflict(_))
    ));
    let mut next = write(3);
    next.digest = "new-package";
    assert_eq!(
        secrets::rotate(&mut tx, own, &next, &crypto).await.unwrap(),
        4
    );
    tx.commit().await?;
    let value:serde_json::Value=sqlx::query_scalar("SELECT data FROM outbox WHERE tenant=$1 AND kind='app.secret_rotated' ORDER BY id DESC LIMIT 1").bind(own.tenant).fetch_one(pool).await?;
    assert_eq!(
        value,
        serde_json::json!({"app":"example","kind":"settings","revision":4,"actor":"fixture-owner"})
    );
    println!(
        "PASS encrypted settings: metadata-only reads, AEAD scope/kind/revision binding, stale package/CAS denial, overflow, outbox rollback, concurrent rotation and locked webhook read"
    );
    Ok(())
}
