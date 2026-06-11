use axum::response::IntoResponse;

/// GET /healthz - Health check endpoint
pub async fn healthz() -> impl IntoResponse {
    "ok"
}
