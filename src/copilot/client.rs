use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use reqwest::cookie::Jar;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::copilot::cookie::{acquire_anon_cookie, build_cookie_header, get_anon_cookie_value};
use crate::copilot::protocol::{ClientEvent, RawServerEvent, ServerEvent};
use crate::error::AppError;

/// The Copilot User-Agent to impersonate
pub const COPILOT_USER_AGENT: &str =
    "CopilotNative/30.0.440527002-prod (Android 9; Xiaomi; Redmi Note 7)";

/// WebSocket endpoint
const WS_CHAT_URL: &str = "wss://copilot.microsoft.com/c/api/chat";

/// The core Copilot client that manages WebSocket connections and interactions
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

    /// Acquire anonymous cookie
    pub async fn get_anon_cookie(&self) -> Result<Arc<Jar>> {
        acquire_anon_cookie(&self.http).await
    }

    /// Establish a WebSocket connection and perform the initialization handshake.
    /// Returns (event_receiver, command_sender, conversation_id).
    pub async fn connect_ws(
        &self,
        jar: &Arc<Jar>,
    ) -> Result<(
        mpsc::Receiver<ServerEvent>,
        mpsc::Sender<ClientEvent>,
        String,
    )> {
        let _cookie_header = build_cookie_header(jar);
        let cookie_value = get_anon_cookie_value(jar)
            .ok_or_else(|| anyhow::anyhow!("no __Host-copilot-anon cookie found"))?;

        // Build WebSocket request
        let host = "copilot.microsoft.com";
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
            .header("Host", host)
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

        // Send initialization event
        let init_event = ClientEvent::default_options();
        let init_json = serde_json::to_string(&init_event)?;
        debug!("copilot send: {init_json}");
        ws_sink
            .send(Message::Text(init_json.into()))
            .await
            .context("failed to send init event")?;

        // Wait for connected event to get conversation_id
        let mut conversation_id;
        let connect_timeout = Duration::from_secs(self.config.conn_timeout);

        loop {
            let msg = timeout(connect_timeout, ws_stream_rx.next())
                .await
                .context("timed out waiting for connected event")?
                .context("WebSocket closed before connected event")?
                .context("WebSocket receive error")?;

            match msg {
                Message::Text(text) => {
                    debug!("copilot event raw: {text}");
                    if let Ok(raw) = serde_json::from_str::<RawServerEvent>(&text) {
                        let event = ServerEvent::from_raw(raw);
                        match event {
                            ServerEvent::Connected {
                                conversation_id: cid,
                            } => {
                                conversation_id = cid;
                                info!("copilot connected, conversation: {conversation_id}");
                                break;
                            }
                            ServerEvent::Error { message, .. } => {
                                return Err(AppError::CopilotUpstream(message).into());
                            }
                            _ => {
                                debug!("ignoring event while waiting for connected");
                            }
                        }
                    }
                }
                Message::Close(_) => {
                    return Err(AppError::CopilotUpstream(
                        "WebSocket closed during handshake".into(),
                    )
                    .into());
                }
                _ => {}
            }
        }

        // Create channels for bidirectional communication
        let (event_tx, event_rx) = mpsc::channel::<ServerEvent>(128);
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientEvent>(64);

        // Spawn read loop
        let debug_mode = self.config.debug;
        tokio::spawn(async move {
            while let Some(msg_result) = ws_stream_rx.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        if debug_mode {
                            debug!("copilot event raw: {text}");
                        }
                        if let Ok(raw) = serde_json::from_str::<RawServerEvent>(&text) {
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
                    Ok(Message::Ping(_data)) => {
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

        // Spawn write loop
        tokio::spawn(async move {
            while let Some(event) = cmd_rx.recv().await {
                match serde_json::to_string(&event) {
                    Ok(json) => {
                        debug!("copilot send: {json}");
                        if let Err(e) = ws_sink.send(Message::Text(json.into())).await {
                            error!("WebSocket write error: {e}");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("failed to serialize client event: {e}");
                    }
                }
            }
        });

        Ok((event_rx, cmd_tx, conversation_id))
    }
}
