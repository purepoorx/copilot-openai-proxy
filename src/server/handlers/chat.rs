use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use tokio::time::timeout;
use tracing::{debug, error, info};

use crate::copilot::image::{process_image_url, upload_image};
use crate::copilot::protocol::{ClientEvent, ServerEvent};
use crate::error::AppError;
use crate::openai::adapter::{
    build_append_text, build_prompt, build_tap_to_reveal, extract_image_urls,
};
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
    let _model = CopilotModel::from_openai_name(model_name)?;

    // Extract session ID from header
    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    info!(
        "chat completion request: model={}, stream={}, session={:?}",
        model_name, request.stream, session_id
    );

    // Get or create session (this creates a new WS connection)
    let request_timeout = Duration::from_secs(state.config.timeout);
    let (event_rx, session) = timeout(
        request_timeout,
        state.session_manager.get_or_create(session_id.as_deref()),
    )
    .await
    .map_err(|_| AppError::CopilotUpstream("session creation timed out".into()))?
    .map_err(|e| AppError::CopilotUpstream(format!("failed to create session: {e}")))?;

    // Process images in the last user message
    let last_msg = request.messages.last().unwrap();
    let image_urls = extract_image_urls(last_msg);

    for url in &image_urls {
        match process_image_url(&state.copilot_client.http, url).await {
            Ok((data, filename, content_type)) => {
                match upload_image(
                    &state.copilot_client.http,
                    &session.cookies,
                    &data,
                    &filename,
                    &content_type,
                )
                .await
                {
                    Ok(attachment_id) => {
                        debug!("image uploaded: {attachment_id}");
                        // Send tap to reveal for the attachment
                        if session
                            .cmd_tx
                            .send(build_tap_to_reveal(attachment_id))
                            .await
                            .is_err()
                        {
                            return Err(AppError::CopilotUpstream(
                                "failed to send tap to reveal".into(),
                            ));
                        }
                    }
                    Err(e) => {
                        error!("upload image failed: {e}");
                        return Err(AppError::CopilotUpstream(format!(
                            "process image_url failed: {e}"
                        )));
                    }
                }
            }
            Err(e) => {
                error!("process image url failed: {e}");
                return Err(AppError::InvalidRequest(format!(
                    "invalid image_url content part: {e}"
                )));
            }
        }
    }

    // Build the text prompt from all messages
    let prompt = build_prompt(&request.messages);
    if prompt.is_empty() {
        return Err(AppError::InvalidRequest("prompt cannot be empty".into()));
    }

    // Send the text to Copilot
    let event_rx = timeout(
        request_timeout,
        send_and_receive(&state, &session, event_rx, prompt),
    )
    .await
    .map_err(|_| AppError::CopilotUpstream("request timed out".into()))?
    .map_err(|e| AppError::CopilotUpstream(format!("copilot error: {e}")))?;

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

/// Send prompt text to Copilot and return the event receiver
async fn send_and_receive(
    _state: &AppState,
    session: &Arc<crate::session::state::SessionState>,
    event_rx: tokio::sync::mpsc::Receiver<ServerEvent>,
    prompt: String,
) -> Result<tokio::sync::mpsc::Receiver<ServerEvent>, String> {
    // Send appendText event
    session
        .cmd_tx
        .send(build_append_text(prompt))
        .await
        .map_err(|_| "failed to send message to copilot".to_string())?;

    Ok(event_rx)
}
