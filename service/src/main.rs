//! Runtime service process entry; it never creates a database connection, issues user keys or loads Core.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = veyndo_app_runtime_service::App::new(veyndo_app_runtime_service::config::load()?)?;
    let listener = tokio::net::TcpListener::bind(app.bind()).await?;
    eprintln!("Veyndo App Runtime listening on {}", listener.local_addr()?);
    axum::serve(listener, veyndo_app_runtime_service::router(app))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
