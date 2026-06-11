use axum::extract::State;
use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::server::AppState;

/// Authentication middleware that checks the Bearer token in Authorization header
pub async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip auth if no API key is configured
    if state.config.api_key.is_empty() {
        return Ok(next.run(request).await);
    }

    // Extract and validate Bearer token
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .unwrap_or("");

    if token != state.config.api_key {
        let error = crate::openai::types::OpenAIError {
            error: crate::openai::types::OpenAIErrorBody {
                message: "invalid API key".to_string(),
                error_type: "invalid_request_error".to_string(),
                code: "401".to_string(),
            },
        };
        return Ok((StatusCode::UNAUTHORIZED, axum::Json(error)).into_response());
    }

    Ok(next.run(request).await)
}
