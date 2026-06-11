use std::convert::Infallible;

use axum::response::sse::{Event, KeepAlive, Sse};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;

use crate::copilot::protocol::ServerEvent;
use crate::openai::types::{ChatCompletionChunk, ChunkChoice, Delta};
use crate::util::id::generate_chat_completion_id;

/// Build an SSE stream that bridges Copilot WebSocket events to OpenAI SSE chunks.
pub fn build_sse_stream(
    mut ws_rx: mpsc::Receiver<ServerEvent>,
    model: String,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let id = generate_chat_completion_id();
    let (sse_tx, sse_rx) = mpsc::channel::<Result<Event, Infallible>>(128);

    tokio::spawn(async move {
        let created = chrono_timestamp();

        // Send initial role chunk
        let role_chunk = ChatCompletionChunk {
            id: id.clone(),
            object: "chat.completion.chunk",
            created,
            model: model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
        };
        if let Ok(json) = serde_json::to_string(&role_chunk) {
            let _ = sse_tx.send(Ok(Event::default().data(json))).await;
        }

        // Stream text deltas from WebSocket
        loop {
            match ws_rx.recv().await {
                Some(ServerEvent::TextDelta { text }) => {
                    if text.is_empty() {
                        continue;
                    }
                    debug!("SSE delta: {}", &text[..text.len().min(80)]);
                    let chunk = ChatCompletionChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk",
                        created,
                        model: model.clone(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: Some(text),
                            },
                            finish_reason: None,
                        }],
                    };
                    if let Ok(json) = serde_json::to_string(&chunk) {
                        if sse_tx.send(Ok(Event::default().data(json))).await.is_err() {
                            debug!("SSE receiver dropped, stopping stream");
                            break;
                        }
                    }
                }
                Some(ServerEvent::TurnComplete) | None => {
                    // Send finish chunk
                    let done_chunk = ChatCompletionChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk",
                        created,
                        model: model.clone(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: None,
                            },
                            finish_reason: Some("stop".to_string()),
                        }],
                    };
                    if let Ok(json) = serde_json::to_string(&done_chunk) {
                        let _ = sse_tx.send(Ok(Event::default().data(json))).await;
                    }
                    // Send [DONE]
                    let _ = sse_tx.send(Ok(Event::default().data("[DONE]"))).await;
                    break;
                }
                Some(ServerEvent::Error { message, .. }) => {
                    debug!("SSE stream error from copilot: {message}");
                    // Send error as a content delta, then finish
                    let err_chunk = ChatCompletionChunk {
                        id: id.clone(),
                        object: "chat.completion.chunk",
                        created,
                        model: model.clone(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: Some(format!("\n\nError: {message}")),
                            },
                            finish_reason: Some("stop".to_string()),
                        }],
                    };
                    if let Ok(json) = serde_json::to_string(&err_chunk) {
                        let _ = sse_tx.send(Ok(Event::default().data(json))).await;
                    }
                    let _ = sse_tx.send(Ok(Event::default().data("[DONE]"))).await;
                    break;
                }
                Some(_) => {
                    // Ignore other event types (image events, unknown, etc.)
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(sse_rx)).keep_alive(KeepAlive::default())
}

/// Collect all text from a WebSocket event stream (for non-streaming responses)
pub async fn collect_full_response(
    mut ws_rx: mpsc::Receiver<ServerEvent>,
) -> Result<String, String> {
    let mut full_text = String::new();

    loop {
        match ws_rx.recv().await {
            Some(ServerEvent::TextDelta { text }) => {
                full_text.push_str(&text);
            }
            Some(ServerEvent::TurnComplete) | None => {
                return Ok(full_text);
            }
            Some(ServerEvent::Error { message, .. }) => {
                return Err(message);
            }
            Some(_) => {}
        }
    }
}

fn chrono_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
