//! Actual independent PostgreSQL persistence: composite scope, atomic handoff, replay, quota, restart and lease fences.
#[path = "cases/basic_storage.rs"]
mod basic_storage;
#[path = "cases/build_versions.rs"]
mod build_version_cases;
#[path = "cases/drafts.rs"]
mod draft_cases;
#[path = "cases/installation.rs"]
mod installation;
#[path = "cases/installation_concurrency.rs"]
mod installation_concurrency;
#[path = "cases/job_control.rs"]
mod job_control_cases;
#[path = "cases/job_history.rs"]
mod job_history_cases;
#[path = "cases/mutations.rs"]
mod mutation_cases;
#[path = "cases/packages.rs"]
mod package_cases;
#[path = "cases/queries.rs"]
mod query_cases;
#[path = "cases/record_history.rs"]
mod record_history_cases;
#[path = "cases/secrets.rs"]
mod secret_cases;
#[tokio::test]
#[ignore = "requires an explicitly selected disposable APP_STORAGE_TEST_DATABASE_URL"]
async fn independent_postgres_contract() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("APP_STORAGE_TEST_DATABASE_URL")?;
    let pool = basic_storage::run(&url).await?;
    mutation_cases::run(&pool).await?;
    query_cases::run(&pool).await?;
    package_cases::run(&pool).await?;
    installation::run(&pool).await?;
    job_control_cases::run(&pool).await?;
    job_history_cases::run(&pool).await?;
    secret_cases::run(&pool).await?;
    build_version_cases::run(&pool).await?;
    draft_cases::run(&pool).await?;
    record_history_cases::run(&pool).await?;
    Ok(())
}
