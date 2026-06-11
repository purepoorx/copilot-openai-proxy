use crate::copilot::protocol::ClientEvent;
use crate::openai::types::{ChatMessage, MessageContent, ContentPart};

/// Convert OpenAI chat messages into a single text prompt for Copilot.
/// Concatenates all messages as context: "role: content\n..."
pub fn build_prompt(messages: &[ChatMessage]) -> String {
    if messages.is_empty() {
        return String::new();
    }

    // If there's only one user message, send it directly
    if messages.len() == 1 {
        return extract_text_from_message(&messages[0]);
    }

    // For multiple messages, build a context string
    let mut parts = Vec::new();
    for msg in messages {
        let text = extract_text_from_message(msg);
        if !text.is_empty() {
            parts.push(format!("{}: {}", msg.role, text));
        }
    }
    parts.join("\n")
}

/// Extract text content from a message (ignoring image parts for now)
pub fn extract_text_from_message(msg: &ChatMessage) -> String {
    match &msg.content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(parts) => {
            let texts: Vec<String> = parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.clone()),
                    ContentPart::ImageUrl { .. } => None, // handled separately
                })
                .collect();
            texts.join("\n")
        }
    }
}

/// Extract image URLs from a message's content parts
pub fn extract_image_urls(msg: &ChatMessage) -> Vec<String> {
    match &msg.content {
        MessageContent::Text(_) => vec![],
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::ImageUrl { image_url } => Some(image_url.url.clone()),
                _ => None,
            })
            .collect(),
    }
}

/// Build a ClientEvent::AppendText from a prompt string
pub fn build_append_text(text: String) -> ClientEvent {
    ClientEvent::append_text(text)
}

/// Build a ClientEvent::TapToReveal for an uploaded image attachment
pub fn build_tap_to_reveal(attachment_id: String) -> ClientEvent {
    ClientEvent::TapToReveal { attachment_id }
}
