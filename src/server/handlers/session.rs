use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use tracing::info;

use crate::error::AppError;
use crate::server::AppState;

/// POST /v1/chat/session - Create a new session (initialize Copilot connection)
pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string());

    info!("creating session: {session_id}");

    let (_event_rx, _msg_tx, conversation_id) = state
        .session_manager
        .create_session()
        .await
        .map_err(|e| AppError::CopilotUpstream(format!("failed to create session: {e}")))?;

    Ok(Json(json!({
        "session_id": session_id,
        "conversation_id": conversation_id,
    })))
}

/// DELETE /v1/chat/session - Delete a session
pub async fn delete_session(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string());

    info!("deleting session: {session_id}");

    // Sessions are now per-request and auto-cleaned, so this is a no-op
    Ok(Json(json!({
        "success": true,
        "message": format!("session {session_id} deleted"),
    })))
}
