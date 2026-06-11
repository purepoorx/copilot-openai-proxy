use uuid::Uuid;

/// Generate a UUID v4 string (no dashes)
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}

/// Generate an OpenAI-compatible chat completion ID
pub fn generate_chat_completion_id() -> String {
    format!("chatcmpl-{}", generate_uuid())
}
