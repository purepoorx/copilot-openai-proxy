use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::cookie::Jar;
use tracing::{debug, info, warn};

use crate::copilot::client::COPILOT_USER_AGENT;
use crate::copilot::cookie::build_cookie_header;

const CONVERSATIONS_HISTORY_URL: &str = "https://copilot.microsoft.com/conversations/history";

/// Delete conversation history on the Copilot server
pub async fn delete_conversation_history(
    http: &reqwest::Client,
    jar: &Arc<Jar>,
    conversation_id: &str,
) -> Result<()> {
    if conversation_id.is_empty() {
        debug!("skipping history deletion: empty conversation ID");
        return Ok(());
    }

    let cookie_header = build_cookie_header(jar);

    let resp = http
        .delete(CONVERSATIONS_HISTORY_URL)
        .header("Cookie", &cookie_header)
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Referer", "https://copilot.microsoft.com/")
        .header("Origin", "https://copilot.microsoft.com")
        .header("X-Copilot-Conversation-Id", conversation_id)
        .header("short-conversation-action", "deleteHistory")
        .send()
        .await
        .context("delete history request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        warn!("delete history returned status {status}: {body}");
    } else {
        info!("conversation history deleted on server: {conversation_id}");
    }

    Ok(())
}
