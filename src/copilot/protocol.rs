use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── Client -> Server Events ───

/// Wrapper for all client events sent to Copilot backend.
/// The Copilot protocol uses "event" as the discriminator field.
#[derive(Debug, Serialize)]
pub struct ClientEventEnvelope {
    pub event: String,
    #[serde(flatten)]
    pub payload: ClientEventPayload,
}

/// Payload variants for client events
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum ClientEventPayload {
    SetOptions(SetOptionsPayload),
    AppendText(AppendTextPayload),
    TapToReveal(TapToRevealPayload),
}

#[derive(Debug, Serialize)]
pub struct SetOptionsPayload {
    #[serde(rename = "timeZone")]
    pub time_zone: String,
    #[serde(rename = "startNewConversation")]
    pub start_new_conversation: bool,
    #[serde(rename = "teenSupportEnabled")]
    pub teen_support_enabled: bool,
    #[serde(rename = "correctPersonalizationSetting")]
    pub correct_personalization_setting: bool,
    #[serde(rename = "deferredDataUseCapable")]
    pub deferred_data_use_capable: bool,
}

#[derive(Debug, Serialize)]
pub struct AppendTextPayload {
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct TapToRevealPayload {
    #[serde(rename = "attachmentId")]
    pub attachment_id: String,
}

/// High-level client event enum for ergonomic use
#[derive(Debug)]
pub enum ClientEvent {
    SetOptions,
    AppendText { text: String },
    TapToReveal { attachment_id: String },
}

impl ClientEvent {
    /// Create the default setOptions initialization event
    pub fn default_options() -> Self {
        ClientEvent::SetOptions
    }

    /// Serialize to JSON string for WebSocket transmission
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let envelope = match self {
            ClientEvent::SetOptions => ClientEventEnvelope {
                event: "setOptions".to_string(),
                payload: ClientEventPayload::SetOptions(SetOptionsPayload {
                    time_zone: "Asia/Shanghai".to_string(),
                    start_new_conversation: true,
                    teen_support_enabled: true,
                    correct_personalization_setting: true,
                    deferred_data_use_capable: true,
                }),
            },
            ClientEvent::AppendText { text } => ClientEventEnvelope {
                event: "appendText".to_string(),
                payload: ClientEventPayload::AppendText(AppendTextPayload {
                    text: text.clone(),
                }),
            },
            ClientEvent::TapToReveal { attachment_id } => ClientEventEnvelope {
                event: "tapToReveal".to_string(),
                payload: ClientEventPayload::TapToReveal(TapToRevealPayload {
                    attachment_id: attachment_id.clone(),
                }),
            },
        };
        serde_json::to_string(&envelope)
    }
}

// ─── Server -> Client Events ───

/// Raw server event from WebSocket - uses "event" as discriminator
#[derive(Debug, Deserialize)]
pub struct RawServerEvent {
    pub event: String,
    #[serde(flatten)]
    pub payload: Value,
}

/// Parsed server events
#[derive(Debug)]
pub enum ServerEvent {
    /// Connection confirmed, returns request/conversation IDs
    Connected {
        request_id: String,
        conversation_id: String,
    },
    /// Text delta (partial response)
    TextDelta {
        text: String,
    },
    /// Turn is complete
    TurnComplete,
    /// Error from upstream
    Error {
        message: String,
        code: Option<String>,
    },
    /// Image generation started
    ImageGenerating,
    /// Image generated successfully
    ImageGenerated {
        url: String,
    },
    /// Image generation failed
    ImageFailed {
        reason: String,
    },
    /// Partial image generated
    ImagePartial {
        content: Vec<u8>,
    },
    /// Unknown event type (for forward compatibility)
    Unknown {
        raw: Value,
    },
}

impl ServerEvent {
    /// Parse a raw server event into a typed ServerEvent
    pub fn from_raw(raw: RawServerEvent) -> Self {
        match raw.event.as_str() {
            "connected" => {
                let request_id = raw
                    .payload
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let conversation_id = raw
                    .payload
                    .get("currentConversationId")
                    .or_else(|| raw.payload.get("current_conversation_id"))
                    .or_else(|| raw.payload.get("conversationId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::Connected {
                    request_id,
                    conversation_id,
                }
            }
            "appendText" => {
                let text = raw
                    .payload
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::TextDelta { text }
            }
            "turnComplete" | "done" => ServerEvent::TurnComplete,
            "error" => {
                let message = raw
                    .payload
                    .get("message")
                    .or_else(|| raw.payload.get("error"))
                    .or_else(|| raw.payload.get("errorCode"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                let code = raw
                    .payload
                    .get("code")
                    .or_else(|| raw.payload.get("errorCode"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ServerEvent::Error { message, code }
            }
            "generatingImage" | "image_generating" => ServerEvent::ImageGenerating,
            "imageGenerated" | "image_generated" => {
                let url = raw
                    .payload
                    .get("url")
                    .or_else(|| raw.payload.get("imageUrl"))
                    .or_else(|| raw.payload.get("image_url"))
                    .or_else(|| raw.payload.get("attachmentUrl"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::ImageGenerated { url }
            }
            "imageFailed" | "image_failed" | "imageGenerationFailed" => {
                let reason = raw
                    .payload
                    .get("reason")
                    .or_else(|| raw.payload.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("image generation failed")
                    .to_string();
                ServerEvent::ImageFailed { reason }
            }
            "partialImageGenerated" | "image_partial" => {
                let content = raw
                    .payload
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.as_bytes().to_vec())
                    .unwrap_or_default();
                ServerEvent::ImagePartial { content }
            }
            _ => ServerEvent::Unknown { raw: raw.payload },
        }
    }
}
