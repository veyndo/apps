//! Standalone authenticated simulation composition; only the identity adapter has HTTP, the shared guest runtime has no host effects.
pub mod config;
pub mod contract;
mod error;
mod execution;
mod identity;
mod mcp;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, State},
    http::header,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;
type Slots = Arc<Mutex<HashMap<(String, Option<String>), usize>>>;
#[derive(Clone)]
pub struct App {
    config: Arc<config::Config>,
    http: reqwest::Client,
    introspection: Arc<Semaphore>,
    execution: Arc<Semaphore>,
    tenants: Slots,
}
impl App {
    pub fn new(config: config::Config) -> Result<Self, &'static str> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .map_err(|_| "Could not configure identity client")?;
        Ok(Self {
            config: Arc::new(config),
            http,
            introspection: Arc::new(Semaphore::new(16)),
            execution: Arc::new(Semaphore::new(4)),
            tenants: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    pub fn bind(&self) -> std::net::SocketAddr {
        self.config.bind
    }
}
pub fn router(app: App) -> Router {
    let protected = Router::new()
        .route("/v1/evaluations", post(evaluate))
        .route("/v1/contract", get(|| async { Json(contract::document()) }))
        .route(
            "/v1/openapi.json",
            get(|| async { Json(contract::openapi()) }),
        )
        .route("/mcp", post(mcp::handle))
        .route_layer(middleware::from_fn_with_state(app.clone(), identity::admit));
    Router::new().route("/health",get(||async{Json(serde_json::json!({"ok":true,"service":"veyndo-app-runtime","version":env!("CARGO_PKG_VERSION")}))}))
        .merge(protected).layer(DefaultBodyLimit::max(131072))
        .layer(middleware::map_response(|mut r:Response|async move{ r.headers_mut().insert(header::CACHE_CONTROL,header::HeaderValue::from_static("no-store"));r.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS,header::HeaderValue::from_static("nosniff"));r }))
        .with_state(app)
}
async fn evaluate(
    State(app): State<App>,
    Extension(principal): Extension<identity::Principal>,
    input: Result<Json<contract::Evaluation>, axum::extract::rejection::JsonRejection>,
) -> error::Result<Response> {
    let input = input
        .map_err(|rejection| {
            if rejection.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                error::Problem::new(
                    rejection.status(),
                    "payload_too_large",
                    "The request exceeds 128 KiB",
                )
            } else if rejection.status() == axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE {
                error::Problem::new(
                    rejection.status(),
                    "unsupported_media_type",
                    "Use application/json",
                )
            } else {
                error::Problem::new(
                    axum::http::StatusCode::BAD_REQUEST,
                    "invalid_request",
                    "Provide a valid evaluation matching /v1/contract",
                )
            }
        })?
        .0;
    Ok(Json(execution::evaluate(&app, principal, input).await?).into_response())
}
