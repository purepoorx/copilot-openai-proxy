use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::openai::types::OpenAIError;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidRequest(String),

    #[error("{0}")]
    CopilotUpstream(String),

    #[error("session not found: {0}")]
    SessionNotFound(String),

    #[error("{0}")]
    UnsupportedModel(String),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tungstenite::Error),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn error_type(&self) -> &'static str {
        match self {
            AppError::InvalidRequest(_) | AppError::UnsupportedModel(_) => {
                "invalid_request_error"
            }
            AppError::CopilotUpstream(_) => "upstream_error",
            AppError::SessionNotFound(_) => "invalid_request_error",
            _ => "server_error",
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            AppError::InvalidRequest(_)
            | AppError::UnsupportedModel(_)
            | AppError::SessionNotFound(_) => StatusCode::BAD_REQUEST,
            AppError::CopilotUpstream(_) => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_openai_error(&self) -> OpenAIError {
        OpenAIError {
            error: crate::openai::types::OpenAIErrorBody {
                message: self.to_string(),
                error_type: self.error_type().to_string(),
                param: None,
                code: None,
            },
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = self.to_openai_error();
        (status, axum::Json(body)).into_response()
    }
}
