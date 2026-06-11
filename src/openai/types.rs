use serde::{Deserialize, Serialize};

// ─── Request Types ───

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ChatCompletionRequest {
    pub model: Option<String>,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrlPart },
}

#[derive(Debug, Deserialize, Clone)]
pub struct ImageUrlPart {
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: Option<String>,
}

// ─── Response Types ───

#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: AssistantMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssistantMessage {
    pub role: String,
    pub content: String,
}

// ─── Streaming Types ───

#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// ─── Image Generation Types ───

#[derive(Debug, Deserialize)]
pub struct ImageGenerationRequest {
    pub model: Option<String>,
    pub prompt: String,
    #[serde(default = "default_n")]
    pub n: u32,
    pub response_format: Option<String>,
}

fn default_n() -> u32 {
    1
}

#[derive(Debug, Serialize)]
pub struct ImageGenerationResponse {
    pub created: i64,
    pub data: Vec<ImageData>,
}

#[derive(Debug, Serialize)]
pub struct ImageData {
    pub url: String,
}

// ─── Model List Types ───

#[derive(Debug, Serialize)]
pub struct ModelListResponse {
    pub object: &'static str,
    pub data: Vec<ModelEntry>,
}

#[derive(Debug, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub object: &'static str,
    pub created: i64,
    pub owned_by: String,
}

// ─── Error Types ───

#[derive(Debug, Serialize)]
pub struct OpenAIError {
    pub error: OpenAIErrorBody,
}

#[derive(Debug, Serialize)]
pub struct OpenAIErrorBody {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}
