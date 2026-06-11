pub mod handlers;
pub mod middleware;

use std::sync::Arc;

use axum::routing::{delete, get, post};
use axum::Router;

use crate::config::Config;
use crate::copilot::client::CopilotClient;
use crate::session::manager::SessionManager;

/// Shared application state passed to all handlers via axum
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub session_manager: Arc<SessionManager>,
    pub copilot_client: Arc<CopilotClient>,
}

/// Build the complete axum Router with all routes and middleware
pub fn build_router(state: AppState) -> Router {
    // Routes that don't require auth
    let public_routes = Router::new()
        .route("/healthz", get(handlers::health::healthz));

    // Routes that require auth
    let api_routes = Router::new()
        .route("/v1/models", get(handlers::models::list_models))
        .route(
            "/v1/chat/completions",
            post(handlers::chat::chat_completions),
        )
        .route(
            "/v1/images/generations",
            post(handlers::images::image_generations),
        )
        .route(
            "/v1/chat/session",
            post(handlers::session::create_session)
                .delete(handlers::session::delete_session),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ));

    public_routes
        .merge(api_routes)
        .with_state(state)
}
