use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;
use tracing::info;

use crate::config::Config;
use crate::copilot::client::CopilotClient;
use crate::copilot::protocol::ServerEvent;

/// Manages Copilot sessions
pub struct SessionManager {
    copilot_client: Arc<CopilotClient>,
}

impl SessionManager {
    pub fn new(_config: Arc<Config>, copilot_client: Arc<CopilotClient>) -> Self {
        Self { copilot_client }
    }

    /// Create a new session by:
    /// 1. HTTP POST to /c/api/start → get cookie + conversation ID
    /// 2. WebSocket connect → wait for connected event
    /// Returns (event_receiver, message_sender, conversation_id)
    pub async fn create_session(
        &self,
    ) -> Result<(
        mpsc::Receiver<ServerEvent>,
        mpsc::Sender<String>,
        String,
    )> {
        info!("creating new copilot session");

        // Step 1: Initialize session via HTTP POST
        let session_init = self.copilot_client.init_session().await?;

        // Step 2: Connect WebSocket
        let (event_rx, msg_tx, cid) = self
            .copilot_client
            .connect_and_start(&session_init)
            .await?;

        Ok((event_rx, msg_tx, cid))
    }

    /// Get a reference to the copilot client
    #[allow(dead_code)]
    pub fn copilot_client(&self) -> &Arc<CopilotClient> {
        &self.copilot_client
    }
}
