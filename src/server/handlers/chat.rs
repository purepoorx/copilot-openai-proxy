use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use tokio::time::timeout;
use tracing::info;

use crate::copilot::client::CopilotClient;
use crate::error::AppError;
use crate::openai::adapter::build_prompt;
use crate::openai::model::CopilotModel;
use crate::openai::stream::{build_sse_stream, collect_full_response};
use crate::openai::types::{
    AssistantMessage, ChatCompletionRequest, ChatCompletionResponse, Choice,
};
use crate::server::AppState;
use crate::util::id::generate_chat_completion_id;

/// POST /v1/chat/completions
pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<axum::response::Response, AppError> {
    // Validate messages
    if request.messages.is_empty() {
        return Err(AppError::InvalidRequest("messages cannot be empty".into()));
    }

    // Parse model
    let model_name = request.model.as_deref().unwrap_or("default");
    let copilot_model = CopilotModel::from_openai_name(model_name)?;

    // Extract session ID from header (for logging)
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    info!(
        "chat completion request: model={}, stream={}, session={:?}",
        model_name, request.stream, session_id
    );

    // Build the text prompt from all messages
    let prompt = build_prompt(&request.messages);
    if prompt.is_empty() {
        return Err(AppError::InvalidRequest("prompt cannot be empty".into()));
    }

    // Create a fresh session for each request
    let request_timeout = Duration::from_secs(state.config.timeout);
    let (event_rx, msg_tx, conversation_id) = timeout(
        request_timeout,
        state.session_manager.create_session(),
    )
    .await
    .map_err(|_| AppError::CopilotUpstream("session creation timed out".into()))?
    .map_err(|e| AppError::CopilotUpstream(format!("failed to create session: {e}")))?;

    // Send setOptions FIRST (original binary sends this before the text message)
    let set_options_msg = CopilotClient::build_ws_set_options();
    timeout(request_timeout, async { msg_tx.send(set_options_msg).await })
        .await
        .map_err(|_| AppError::CopilotUpstream("setOptions send timed out".into()))?
        .map_err(|_| AppError::CopilotUpstream("failed to send setOptions".into()))?;

    // Then send the text message
    let ws_message = CopilotClient::build_ws_message(
        copilot_model.to_copilot_mode(),
        &conversation_id,
        &prompt,
    );

    // Send text message
    timeout(request_timeout, async { msg_tx.send(ws_message).await })
        .await
        .map_err(|_| AppError::CopilotUpstream("message send timed out".into()))?
        .map_err(|_| AppError::CopilotUpstream("failed to send message".into()))?;

    // Return streaming or non-streaming response
    if request.stream {
        Ok(build_sse_stream(event_rx, model_name.to_string()).into_response())
    } else {
        let full_text = timeout(
            request_timeout,
            collect_full_response(event_rx),
        )
        .await
        .map_err(|_| AppError::CopilotUpstream("response timed out".into()))?
        .map_err(|msg| AppError::CopilotUpstream(msg))?;

        let id = generate_chat_completion_id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let response = ChatCompletionResponse {
            id,
            object: "chat.completion",
            created: now,
            model: model_name.to_string(),
            choices: vec![Choice {
                index: 0,
                message: AssistantMessage {
                    role: "assistant".to_string(),
                    content: full_text,
                },
                finish_reason: Some("stop".to_string()),
            }],
        };

        Ok(Json(response).into_response())
    }
}
