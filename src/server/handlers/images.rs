use std::time::Duration;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use tokio::time::timeout;
use tracing::{debug, info};

use crate::copilot::client::CopilotClient;
use crate::copilot::protocol::ServerEvent;
use crate::error::AppError;
use crate::openai::model::CopilotModel;
use crate::openai::types::{ImageGenerationRequest, ImageGenerationResponse, ImageData};
use crate::server::AppState;

/// POST /v1/images/generations
pub async fn image_generations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ImageGenerationRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate n
    if request.n != 1 {
        return Err(AppError::InvalidRequest(
            "n must be 1; only n=1 is available".into(),
        ));
    }

    // Validate response_format
    if let Some(ref fmt) = request.response_format {
        if fmt != "url" {
            return Err(AppError::InvalidRequest(format!(
                "response_format {fmt:?} is not supported; only url is available"
            )));
        }
    }

    // Validate prompt
    if request.prompt.is_empty() {
        return Err(AppError::InvalidRequest("prompt cannot be empty".into()));
    }

    let model_name = request.model.as_deref().unwrap_or("default");
    let copilot_model = CopilotModel::from_openai_name(model_name)?;

    info!("image generation request: model={}, prompt_len={}", model_name, request.prompt.len());

    let request_timeout = Duration::from_secs(state.config.timeout);

    // Create a fresh session
    let (mut event_rx, msg_tx, conversation_id) = timeout(
        request_timeout,
        state.session_manager.create_session(),
    )
    .await
    .map_err(|_| AppError::CopilotUpstream("session creation timed out".into()))?
    .map_err(|e| AppError::CopilotUpstream(format!("failed to create session: {e}")))?;

    // Build and send the image generation prompt
    let ws_message = CopilotClient::build_ws_message(
        copilot_model.to_copilot_mode(),
        &conversation_id,
        &request.prompt,
    );

    timeout(request_timeout, async { msg_tx.send(ws_message).await })
        .await
        .map_err(|_| AppError::CopilotUpstream("message send timed out".into()))?
        .map_err(|_| AppError::CopilotUpstream("failed to send prompt".into()))?;

    // Wait for image_generated event
    let mut image_url = String::new();

    let result = timeout(request_timeout, async {
        loop {
            match event_rx.recv().await {
                Some(ServerEvent::ImageGenerated { url }) => {
                    debug!("image generated: {url}");
                    image_url = url;
                    return Ok(());
                }
                Some(ServerEvent::ImageFailed { reason }) => {
                    return Err(AppError::CopilotUpstream(format!(
                        "image generation failed: {reason}"
                    )));
                }
                Some(ServerEvent::Error { message, .. }) => {
                    return Err(AppError::CopilotUpstream(message));
                }
                Some(ServerEvent::TurnComplete) | None => {
                    if image_url.is_empty() {
                        return Err(AppError::CopilotUpstream(
                            "copilot finished without imageGenerated event".into(),
                        ));
                    }
                    return Ok(());
                }
                Some(_) => {}
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            Ok(Json(ImageGenerationResponse {
                created: now,
                data: vec![ImageData { url: image_url }],
            }))
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err(AppError::CopilotUpstream(
            "image generation timed out".into(),
        )),
    }
}
