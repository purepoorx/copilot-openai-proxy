use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::copilot::protocol::{RawServerEvent, ServerEvent};
use crate::error::AppError;

/// The Copilot User-Agent to impersonate
pub const COPILOT_USER_AGENT: &str =
    "CopilotNative/30.0.440527002-prod (Android 9; Xiaomi; Redmi Note 7)";

/// WebSocket endpoint
const WS_CHAT_URL: &str = "wss://copilot.microsoft.com/c/api/chat";

/// HTTP POST endpoint for session initialization
const API_START_URL: &str = "https://copilot.microsoft.com/c/api/start";

/// Pre-serialized setOptions JSON body (matches original binary exactly)
const SET_OPTIONS_BODY: &str = r#"{"timeZone":"Asia/Shanghai","startNewConversation":true,"teenSupportEnabled":true,"correctPersonalizationSetting":true,"deferredDataUseCapable":true}"#;

/// Response from /c/api/start
#[derive(Debug, Deserialize)]
pub struct StartResponse {
    #[serde(rename = "isBlocked", default)]
    pub is_blocked: bool,
    #[serde(rename = "currentConversationId", default)]
    pub current_conversation_id: String,
}

/// The core Copilot client
pub struct CopilotClient {
    pub http: reqwest::Client,
    pub config: Arc<Config>,
}

impl CopilotClient {
    /// Create a new CopilotClient with the given config
    pub fn new(config: Arc<Config>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(COPILOT_USER_AGENT)
            .timeout(Duration::from_secs(config.timeout))
            .connect_timeout(Duration::from_secs(config.conn_timeout))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self { http, config })
    }

    /// Initialize a session by POSTing to /c/api/start.
    /// Returns: (anon_cookie_value, conversation_id)
    pub async fn init_session(&self) -> Result<SessionInit> {
        info!("initializing copilot session via HTTP POST");

        let resp = self
            .http
            .post(API_START_URL)
            .header("Content-Type", "application/json")
            .header("Referer", "https://copilot.microsoft.com")
            .header("Accept", "application/json")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .body(SET_OPTIONS_BODY)
            .send()
            .await
            .context("copilot start request failed")?;

        let status = resp.status();
        debug!("copilot start response: {status}");

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "copilot start returned status {status}: {body}"
            ));
        }

        // Extract X-Copilot-Conversation-Id header
        let conversation_id = resp
            .headers()
            .get("X-Copilot-Conversation-Id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Extract __Host-copilot-anon cookie from Set-Cookie headers
        let mut anon_cookie = None;
        for cookie_header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(cookie_str) = cookie_header.to_str() {
                if let Some(value) = cookie_str.strip_prefix("__Host-copilot-anon=") {
                    let value = value.split(';').next().unwrap_or("").to_string();
                    anon_cookie = Some(value);
                    debug!("acquired __Host-copilot-anon cookie from start response");
                }
            }
        }

        // Also check response body
        let body_text = resp.text().await.unwrap_or_default();
        debug!("copilot start body: {body_text}");

        if let Ok(start_resp) = serde_json::from_str::<StartResponse>(&body_text) {
            if start_resp.is_blocked {
                warn!("copilot start reports user is blocked");
            }
        }

        let cookie = anon_cookie
            .ok_or_else(|| anyhow::anyhow!("copilot start did not return __Host-copilot-anon cookie"))?;

        info!("acquired copilot session cookie, conversation: {conversation_id}");

        Ok(SessionInit {
            anon_cookie: cookie,
            conversation_id,
        })
    }

    /// Connect WebSocket and perform the full session handshake.
    /// Returns (event_receiver, conversation_id).
    pub async fn connect_and_start(
        &self,
        session: &SessionInit,
    ) -> Result<(mpsc::Receiver<ServerEvent>, mpsc::Sender<String>, String)> {
        let cookie_value = &session.anon_cookie;
        let conversation_id = session.conversation_id.clone();

        // Build WebSocket request
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(WS_CHAT_URL)
            .header("User-Agent", COPILOT_USER_AGENT)
            .header("Cookie", format!("__Host-copilot-anon={cookie_value}"))
            .header("Origin", "https://copilot.microsoft.com")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .header("Host", "copilot.microsoft.com")
            .body(())
            .context("failed to build WebSocket request")?;

        // Connect with timeout
        let conn_timeout = Duration::from_secs(self.config.conn_timeout);
        let (ws_stream, _response) = timeout(conn_timeout, async {
            tokio_tungstenite::connect_async(request).await
        })
        .await
        .context("WebSocket connection timed out")?
        .context("WebSocket connection failed")?;

        info!("WebSocket connected to copilot");

        let (mut ws_sink, mut ws_stream_rx) = ws_stream.split();

        // Wait for connected event
        let connect_timeout = Duration::from_secs(self.config.conn_timeout);
        let msg = timeout(connect_timeout, ws_stream_rx.next())
            .await
            .context("timed out waiting for connected event")?
            .context("WebSocket closed before connected event")?
            .context("WebSocket receive error")?;

        let final_cid = match msg {
            Message::Text(text) => {
                let text_str = text.to_string();
                debug!("copilot connected event: {text_str}");
                if let Ok(raw) = serde_json::from_str::<RawServerEvent>(&text_str) {
                    match ServerEvent::from_raw(raw) {
                        ServerEvent::Connected {
                            conversation_id: cid,
                            ..
                        } => {
                            if !cid.is_empty() { cid } else { conversation_id }
                        }
                        ServerEvent::Error { message, .. } => {
                            return Err(AppError::CopilotUpstream(message).into());
                        }
                        _ => conversation_id,
                    }
                } else {
                    conversation_id
                }
            }
            Message::Close(_) => {
                return Err(AppError::CopilotUpstream(
                    "WebSocket closed during handshake".into(),
                )
                .into());
            }
            _ => conversation_id,
        };

        info!("copilot session ready, conversation: {final_cid}");

        // Create channels
        let (event_tx, event_rx) = mpsc::channel::<ServerEvent>(128);
        let (msg_tx, mut msg_rx) = mpsc::channel::<String>(1);

        // Spawn read loop
        let debug_mode = self.config.debug;
        tokio::spawn(async move {
            while let Some(msg_result) = ws_stream_rx.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str = text.to_string();
                        if debug_mode {
                            debug!("copilot event raw: {text_str}");
                        }
                        if let Ok(raw) = serde_json::from_str::<RawServerEvent>(&text_str) {
                            let event = ServerEvent::from_raw(raw);
                            if event_tx.send(event).await.is_err() {
                                debug!("event receiver dropped, stopping read loop");
                                break;
                            }
                        }
                    }
                    Ok(Message::Close(_)) => {
                        info!("WebSocket closed by server");
                        break;
                    }
                    Ok(Message::Ping(_)) => {
                        debug!("received ping, pong handled by tungstenite");
                    }
                    Err(e) => {
                        warn!("WebSocket read error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Spawn write loop - receives JSON strings and sends them over WebSocket
        tokio::spawn(async move {
            while let Some(json) = msg_rx.recv().await {
                if debug_mode {
                    debug!("copilot send: {json}");
                }
                if let Err(e) = ws_sink.send(Message::Text(json.into())).await {
                    warn!("WebSocket write error: {e}");
                    break;
                }
            }
            // Keep ws_sink alive until msg_rx is closed
        });

        Ok((event_rx, msg_tx, final_cid))
    }

    /// Send a user message to the Copilot WebSocket.
    /// The message format is: {"model":"...", "conversationId":"...", "text":"..."}
    pub fn build_ws_message(model: &str, conversation_id: &str, text: &str) -> String {
        serde_json::json!({
            "model": model,
            "conversationId": conversation_id,
            "text": text
        })
        .to_string()
    }
}

/// Session initialization result from HTTP POST /c/api/start
pub struct SessionInit {
    pub anon_cookie: String,
    pub conversation_id: String,
}
