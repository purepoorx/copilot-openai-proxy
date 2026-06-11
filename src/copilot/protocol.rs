use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── Client -> Server Events ───

/// Events sent from our proxy to the Copilot backend
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ClientEvent {
    /// Initialize connection settings
    #[serde(rename = "setOptions")]
    SetOptions {
        #[serde(rename = "timeZone")]
        time_zone: String,
        #[serde(rename = "startNewConversation")]
        start_new_conversation: bool,
        #[serde(rename = "teenSupportEnabled")]
        teen_support_enabled: bool,
        #[serde(rename = "correctPersonalizationSetting")]
        correct_personalization_setting: bool,
        #[serde(rename = "deferredDataUseCapable")]
        deferred_data_use_capable: bool,
    },

    /// Send user text message
    #[serde(rename = "appendText")]
    AppendText { text: String },

    /// Multimodal content reveal (images, etc.)
    #[serde(rename = "tapToReveal")]
    TapToReveal {
        #[serde(rename = "attachment_id")]
        attachment_id: String,
    },
}

impl ClientEvent {
    /// Create the default setOptions initialization event
    pub fn default_options() -> Self {
        ClientEvent::SetOptions {
            time_zone: "Asia/Shanghai".to_string(),
            start_new_conversation: true,
            teen_support_enabled: true,
            correct_personalization_setting: true,
            deferred_data_use_capable: true,
        }
    }
}

// ─── Server -> Client Events ───

/// Events received from the Copilot backend
#[derive(Debug, Deserialize)]
pub struct RawServerEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub payload: Value,
}

/// Parsed server events
#[derive(Debug)]
pub enum ServerEvent {
    /// Connection confirmed, returns conversation ID
    Connected {
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
        url: String,
    },
    /// Unknown event type (for forward compatibility)
    Unknown {
        raw: Value,
    },
}

impl ServerEvent {
    /// Parse a raw server event into a typed ServerEvent
    pub fn from_raw(raw: RawServerEvent) -> Self {
        match raw.event_type.as_str() {
            "connected" => {
                let conversation_id = raw
                    .payload
                    .get("currentConversationId")
                    .or_else(|| raw.payload.get("current_conversation_id"))
                    .or_else(|| raw.payload.get("conversationId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::Connected { conversation_id }
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
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                let code = raw
                    .payload
                    .get("code")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                ServerEvent::Error { message, code }
            }
            "image_generating" => ServerEvent::ImageGenerating,
            "image_generated" => {
                let url = raw
                    .payload
                    .get("url")
                    .or_else(|| raw.payload.get("imageUrl"))
                    .or_else(|| raw.payload.get("image_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::ImageGenerated { url }
            }
            "image_failed" | "image_generation_failed" => {
                let reason = raw
                    .payload
                    .get("reason")
                    .or_else(|| raw.payload.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("image generation failed")
                    .to_string();
                ServerEvent::ImageFailed { reason }
            }
            "image_partial" | "partialImageGenerated" => {
                let url = raw
                    .payload
                    .get("url")
                    .or_else(|| raw.payload.get("imageUrl"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                ServerEvent::ImagePartial { url }
            }
            _ => ServerEvent::Unknown { raw: raw.payload },
        }
    }
}
