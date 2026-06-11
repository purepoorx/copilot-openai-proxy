use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use crate::error::AppError;
use crate::server::AppState;

/// POST /v1/chat/session - Create a new session
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

    let (_event_rx, session) = state
        .session_manager
        .get_or_create(Some(&session_id))
        .await
        .map_err(|e| AppError::CopilotUpstream(format!("failed to create session: {e}")))?;

    Ok(Json(json!({
        "session_id": session.id,
        "conversation_id": session.conversation_id,
    })))
}

/// DELETE /v1/chat/session - Delete a session and its history
pub async fn delete_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "default".to_string());

    info!("deleting session: {session_id}");

    state
        .session_manager
        .delete(&session_id)
        .await
        .map_err(|e| AppError::CopilotUpstream(format!("failed to delete session: {e}")))?;

    Ok(Json(json!({
        "success": true,
        "message": format!("session {session_id} deleted"),
    })))
}
