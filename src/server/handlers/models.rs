use axum::response::IntoResponse;
use axum::Json;

use crate::openai::model::AVAILABLE_MODELS;
use crate::openai::types::{ModelEntry, ModelListResponse};

/// GET /v1/models - List available models
pub async fn list_models() -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let models: Vec<ModelEntry> = AVAILABLE_MODELS
        .iter()
        .map(|name| ModelEntry {
            id: name.to_string(),
            object: "model",
            created: now,
            owned_by: "copilot".to_string(),
        })
        .collect();

    Json(ModelListResponse {
        object: "list",
        data: models,
    })
}
