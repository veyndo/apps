//! Independent App Data read composition root; OAuth, signed tenant SQL and declared-field projection share owners with Veyndo.
pub mod config;
pub mod contract;
mod database;
mod error;
mod identity;
mod install_contract;
mod install_hooks;
mod install_policy;
mod install_role;
mod installer;
mod job_contract;
mod job_host;
mod job_role;
mod job_service;
mod management;
mod mcp;
mod portable;
mod reader;
mod settings;
mod settings_contract;
mod settings_role;
mod write_contract;
mod write_role;
mod writer;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State},
    http::{StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::sync::Arc;
use tokio::sync::Semaphore;
#[derive(Clone)]
pub struct App {
    config: Arc<config::Config>,
    pool: sqlx::PgPool,
    http: reqwest::Client,
    introspection: Arc<Semaphore>,
    execution: Arc<Semaphore>,
}
impl App {
    pub async fn new(config: config::Config, role: &str) -> Result<Self, &'static str> {
        let pool = database::connect(
            &config.database,
            role,
            config.writes,
            config.settings.is_some(),
            config.installs.is_some(),
            config.jobs,
        )
        .await?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|_| "Could not configure identity client")?;
        Ok(Self {
            config: Arc::new(config),
            pool,
            http,
            introspection: Arc::new(Semaphore::new(16)),
            execution: Arc::new(Semaphore::new(8)),
        })
    }
    pub fn bind(&self) -> std::net::SocketAddr {
        self.config.identity.bind
    }
}
pub fn router(app: App) -> Router {
    let writes = app.config.writes;
    let protected = Router::new()
        .route("/v1/records/query", post(read))
        .route(
            "/v1/contract",
            get(|State(app): State<App>| async move {
                Json(contract::document(
                    app.config.writes,
                    app.config.settings.is_some(),
                    app.config.installs.is_some(),
                    app.config.jobs,
                ))
            }),
        )
        .route(
            "/v1/openapi.json",
            get(|State(app): State<App>| async move {
                Json(contract::openapi(
                    app.config.writes,
                    app.config.settings.is_some(),
                    app.config.installs.is_some(),
                    app.config.jobs,
                ))
            }),
        )
        .route("/v1/records/write", post(write))
        .route("/v1/records/history", post(history))
        .merge(settings::router())
        .merge(installer::router())
        .merge(job_service::router())
        .route("/mcp", post(mcp::handle))
        .route_layer(middleware::from_fn_with_state(app.clone(), identity::admit));
    Router::new().route("/health",get(||async{Json(serde_json::json!({"ok":true,"service":"veyndo-app-data","version":env!("CARGO_PKG_VERSION")}))})).merge(protected).layer(DefaultBodyLimit::max(if writes {131072}else{16384})).layer(middleware::map_response(|mut r:Response|async move {r.headers_mut().insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS,header::HeaderValue::from_static("nosniff"));r})).with_state(app)
}
async fn read(
    State(app): State<App>,
    Extension(p): Extension<identity::Principal>,
    body: Result<Json<contract::Read>, axum::extract::rejection::JsonRejection>,
) -> error::Result<Response> {
    let input = body
        .map_err(|r| {
            error::Problem::new(
                if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    r.status()
                } else {
                    StatusCode::BAD_REQUEST
                },
                "invalid_query",
                "Use application/json and the bounded App record query contract",
            )
        })?
        .0;
    Ok(Json(reader::read(&app, p, input, false).await?).into_response())
}

async fn write(
    State(app): State<App>,
    Extension(p): Extension<identity::Principal>,
    body: Result<Json<write_contract::Write>, axum::extract::rejection::JsonRejection>,
) -> error::Result<Response> {
    let input = body
        .map_err(|r| {
            error::Problem::new(
                if r.status() == StatusCode::PAYLOAD_TOO_LARGE {
                    r.status()
                } else {
                    StatusCode::BAD_REQUEST
                },
                "invalid_write",
                "Use the approved package-bound write contract",
            )
        })?
        .0;
    Ok(Json(writer::write(&app, p, input, false).await?).into_response())
}
async fn history(
    State(app): State<App>,
    Extension(p): Extension<identity::Principal>,
    body: Result<Json<write_contract::History>, axum::extract::rejection::JsonRejection>,
) -> error::Result<Response> {
    let input = body
        .map_err(|_| {
            error::Problem::new(
                StatusCode::BAD_REQUEST,
                "invalid_history",
                "Use the App record history contract",
            )
        })?
        .0;
    Ok(Json(writer::history(&app, p, input, false).await?).into_response())
}
