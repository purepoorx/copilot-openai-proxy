use std::sync::Arc;
use std::time::Instant;

use reqwest::cookie::Jar;
use tokio::sync::mpsc;

use crate::copilot::protocol::ClientEvent;

/// State for a single Copilot session
pub struct SessionState {
    /// Unique session identifier
    pub id: String,
    /// Channel to send events to the Copilot backend
    pub cmd_tx: mpsc::Sender<ClientEvent>,
    /// Conversation ID from the Copilot backend
    pub conversation_id: String,
    /// Cookie jar for this session
    pub cookies: Arc<Jar>,
    /// When this session was created
    pub created_at: Instant,
    /// When this session expires
    pub expires_at: Instant,
}

impl SessionState {
    /// Check if this session has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}
