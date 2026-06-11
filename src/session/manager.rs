use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::Config;
use crate::copilot::client::CopilotClient;
use crate::copilot::history::delete_conversation_history;
use crate::copilot::protocol::ServerEvent;
use crate::session::state::SessionState;

/// Manages a pool of Copilot sessions with lifecycle management
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<SessionState>>>>,
    config: Arc<Config>,
    copilot_client: Arc<CopilotClient>,
}

impl SessionManager {
    pub fn new(config: Arc<Config>, copilot_client: Arc<CopilotClient>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
            copilot_client,
        }
    }

    /// Get an existing session or create a new one.
    /// Returns (event_receiver, session_state_arc).
    /// Note: The event_receiver is taken from the session on first use.
    pub async fn get_or_create(
        &self,
        session_id: Option<&str>,
    ) -> Result<(
        tokio::sync::mpsc::Receiver<ServerEvent>,
        Arc<SessionState>,
    )> {
        let sid = session_id.unwrap_or("default");

        // Check if we have an existing valid session
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(sid) {
                if !session.is_expired() {
                    debug!("reusing session: {sid}");
                    // For existing sessions, we return a dummy receiver
                    // The actual event stream is managed per-request
                    let (_event_tx, event_rx) = tokio::sync::mpsc::channel(128);
                    return Ok((event_rx, Arc::clone(session)));
                }
            }
        }

        // Create a new session
        self.create_session(sid).await
    }

    /// Create a fresh session with Copilot
    async fn create_session(
        &self,
        session_id: &str,
    ) -> Result<(
        tokio::sync::mpsc::Receiver<ServerEvent>,
        Arc<SessionState>,
    )> {
        info!("creating new session: {session_id}");

        // Evict oldest if at capacity
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.config.max_sessions {
                drop(sessions);
                self.evict_oldest().await;
            }
        }

        // Acquire cookie
        let jar = self.copilot_client.get_anon_cookie().await?;

        // Connect WebSocket
        let (event_rx, cmd_tx, conversation_id) =
            self.copilot_client.connect_ws(&jar).await?;

        let ttl = Duration::from_secs(self.config.session_ttl);
        let session = Arc::new(SessionState {
            id: session_id.to_string(),
            cmd_tx,
            conversation_id,
            cookies: jar,
            created_at: Instant::now(),
            expires_at: Instant::now() + ttl,
        });

        // Store in map
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id.to_string(), Arc::clone(&session));
        }

        Ok((event_rx, session))
    }

    /// Delete a session and optionally clean up server-side history
    pub async fn delete(&self, session_id: &str) -> Result<()> {
        let session = {
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id)
        };

        if let Some(session) = session {
            info!("deleting session: {session_id}");
            // Best effort: delete conversation history on server
            if let Err(e) = delete_conversation_history(
                &self.copilot_client.http,
                &session.cookies,
                &session.conversation_id,
            )
            .await
            {
                warn!("failed to delete history for {session_id}: {e}");
            }
        }

        Ok(())
    }

    /// Spawn a background task to periodically clean up expired sessions
    pub fn spawn_cleanup_task(self: Arc<Self>) {
        let interval = Duration::from_secs(self.config.cleanup_interval);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                self.cleanup_expired().await;
            }
        });
    }

    /// Remove all expired sessions
    async fn cleanup_expired(&self) {
        let expired: Vec<(String, Arc<SessionState>)> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|(_, s)| s.is_expired())
                .map(|(k, v)| (k.clone(), Arc::clone(v)))
                .collect()
        };

        for (sid, session) in &expired {
            debug!("cleaning up expired session: {sid}");
            // Best effort: clean server history
            let _ = delete_conversation_history(
                &self.copilot_client.http,
                &session.cookies,
                &session.conversation_id,
            )
            .await;
        }

        if !expired.is_empty() {
            let mut sessions = self.sessions.write().await;
            for (sid, _) in &expired {
                sessions.remove(sid);
            }
            info!("cleaned up {} expired sessions", expired.len());
        }
    }

    /// Evict the oldest session when max_sessions is exceeded
    async fn evict_oldest(&self) {
        let oldest = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .min_by_key(|(_, s)| s.created_at)
                .map(|(k, v)| (k.clone(), Arc::clone(v)))
        };

        if let Some((sid, session)) = oldest {
            debug!("evicting oldest session: {sid}");
            let _ = delete_conversation_history(
                &self.copilot_client.http,
                &session.cookies,
                &session.conversation_id,
            )
            .await;

            let mut sessions = self.sessions.write().await;
            sessions.remove(&sid);
        }
    }

    /// Get a reference to the copilot client
    #[allow(dead_code)]
    pub fn copilot_client(&self) -> &Arc<CopilotClient> {
        &self.copilot_client
    }
}
