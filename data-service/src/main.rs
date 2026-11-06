//! Read-only service entry; schema installation and database key provisioning are separate operator actions.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = veyndo_app_data_service::config::Config::load()?;
    let role = std::env::var("APP_DATA_DATABASE_ROLE").map_err(|_| "Missing database role")?;
    let app = veyndo_app_data_service::App::new(config, &role).await?;
    let listener = tokio::net::TcpListener::bind(app.bind()).await?;
    eprintln!("Veyndo App Data listening on {}", listener.local_addr()?);
    axum::serve(listener, veyndo_app_data_service::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
