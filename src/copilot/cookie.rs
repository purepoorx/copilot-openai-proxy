use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::Url;
use tracing::{debug, info};

const COPILOT_BASE_URL: &str = "https://copilot.microsoft.com";

/// Acquire an anonymous cookie by visiting the Copilot homepage.
/// Returns a cookie jar containing the `__Host-copilot-anon` cookie.
pub async fn acquire_anon_cookie(http: &reqwest::Client) -> Result<Arc<Jar>> {
    info!("acquiring copilot anon cookie");

    let resp = http
        .get(COPILOT_BASE_URL)
        .send()
        .await
        .context("failed to reach copilot.microsoft.com")?;

    let status = resp.status();
    debug!("cookie acquisition response: {status}");

    // The cookie store on the client should have captured __Host-copilot-anon
    // We need to extract it and put it in our own jar
    let jar = Arc::new(Jar::default());

    // Extract cookies from the response Set-Cookie headers
    for cookie_header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            let url = Url::parse(COPILOT_BASE_URL)?;
            jar.add_cookie_str(cookie_str, &url);
            debug!("captured cookie: {}", cookie_str.split(';').next().unwrap_or(""));
        }
    }

    // Also consume the response body to complete the request
    let _ = resp.text().await;

    Ok(jar)
}

/// Extract the __Host-copilot-anon cookie value from a jar
pub fn get_anon_cookie_value(jar: &Arc<Jar>) -> Option<String> {
    let url = Url::parse(COPILOT_BASE_URL).ok()?;
    let cookies = jar.cookies(&url)?;
    let cookie_str = cookies.to_str().ok()?;

    for part in cookie_str.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("__Host-copilot-anon=") {
            return Some(value.to_string());
        }
    }
    None
}

/// Build a Cookie header string from a jar for the Copilot domain
pub fn build_cookie_header(jar: &Arc<Jar>) -> String {
    let url = match Url::parse(COPILOT_BASE_URL) {
        Ok(u) => u,
        Err(_) => return String::new(),
    };
    match jar.cookies(&url) {
        Some(c) => c.to_str().unwrap_or("").to_string(),
        None => String::new(),
    }
}
