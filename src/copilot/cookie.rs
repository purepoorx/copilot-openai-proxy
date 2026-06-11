use std::sync::Arc;

use anyhow::{Context, Result};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::Url;
use tracing::{debug, info, warn};

const COPILOT_BASE_URL: &str = "https://copilot.microsoft.com";

/// Acquire an anonymous cookie by visiting the Copilot homepage.
/// Follows redirects manually to capture cookies at each hop.
/// Returns a cookie jar containing the `__Host-copilot-anon` cookie.
pub async fn acquire_anon_cookie(http: &reqwest::Client) -> Result<Arc<Jar>> {
    info!("acquiring copilot anon cookie");

    let jar = Arc::new(Jar::default());
    let base_url = Url::parse(COPILOT_BASE_URL)?;

    // Build a client that does NOT follow redirects, so we can capture
    // cookies from every hop in the redirect chain.
    let no_redirect_client = reqwest::Client::builder()
        .user_agent(super::client::COPILOT_USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build no-redirect HTTP client")?;

    let mut current_url = COPILOT_BASE_URL.to_string();

    // Follow up to 10 redirects, collecting cookies at each step
    for hop in 0..10 {
        debug!("cookie acquisition hop {hop}: {current_url}");

        let resp = no_redirect_client
            .get(&current_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .header("Cookie", build_cookie_header(&jar))
            .send()
            .await
            .with_context(|| format!("failed to reach {current_url}"))?;

        let status = resp.status();
        debug!("hop {hop} response: {status}");

        // Capture all Set-Cookie headers from this response
        for cookie_header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
            if let Ok(cookie_str) = cookie_header.to_str() {
                let url = Url::parse(&current_url).unwrap_or_else(|_| base_url.clone());
                jar.add_cookie_str(cookie_str, &url);
                let name = cookie_str.split(';').next().unwrap_or("").split('=').next().unwrap_or("");
                debug!("hop {hop} captured cookie: {name}");
            }
        }

        // Check if we already have the target cookie
        if get_anon_cookie_value(&jar).is_some() {
            info!("acquired __Host-copilot-anon cookie after {hop} hops");
            return Ok(jar);
        }

        // Follow redirect if present
        if status.is_redirection() {
            if let Some(location) = resp.headers().get(reqwest::header::LOCATION) {
                let location_str = location.to_str().unwrap_or("");
                // Handle relative and absolute URLs
                current_url = if location_str.starts_with("http") {
                    location_str.to_string()
                } else if location_str.starts_with('/') {
                    let url = Url::parse(&current_url)?;
                    format!("{}://{}{}", url.scheme(), url.host_str().unwrap_or("copilot.microsoft.com"), location_str)
                } else {
                    format!("{current_url}/{location_str}")
                };
                debug!("hop {hop} redirect to: {current_url}");
                continue;
            }
        }

        // If not a redirect, try to parse the body for meta refresh or JS redirects
        if status.is_success() {
            let body = resp.text().await.unwrap_or_default();

            // Check for meta refresh: <meta http-equiv="refresh" content="0;url=...">
            if let Some(url) = extract_meta_refresh(&body) {
                debug!("hop {hop} found meta refresh: {url}");
                current_url = if url.starts_with("http") {
                    url
                } else {
                    format!("{COPILOT_BASE_URL}/{url}")
                };
                continue;
            }

            // If body contains a reference to the anon cookie endpoint, try it
            if body.contains("copilot-anon") || body.contains("/c/api") {
                debug!("hop {hop} body mentions copilot, trying /c/api/start");
                current_url = format!("{COPILOT_BASE_URL}/c/api/start");
                continue;
            }
        }

        // No more redirects, break
        break;
    }

    // Final attempt: try the /c/api/start endpoint directly
    if get_anon_cookie_value(&jar).is_none() {
        debug!("trying /c/api/start endpoint for cookie");
        let resp = no_redirect_client
            .get(format!("{COPILOT_BASE_URL}/c/api/start"))
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Cookie", build_cookie_header(&jar))
            .send()
            .await;

        if let Ok(resp) = resp {
            for cookie_header in resp.headers().get_all(reqwest::header::SET_COOKIE) {
                if let Ok(cookie_str) = cookie_header.to_str() {
                    jar.add_cookie_str(cookie_str, &base_url);
                }
            }
        }
    }

    if get_anon_cookie_value(&jar).is_some() {
        info!("acquired __Host-copilot-anon cookie");
    } else {
        warn!("failed to acquire __Host-copilot-anon cookie");
    }

    Ok(jar)
}

/// Extract URL from meta refresh tag in HTML body
fn extract_meta_refresh(body: &str) -> Option<String> {
    // Look for <meta http-equiv="refresh" content="0;url=...">
    let lower = body.to_lowercase();
    if let Some(pos) = lower.find("http-equiv=\"refresh\"") {
        // Find the content attribute
        let start = lower[pos..].find("content=\"")?;
        let content_start = pos + start + 9; // len("content=\"")
        let content_end = lower[content_start..].find('"')?;
        let content = &body[content_start..content_start + content_end];
        // Parse "0;url=..."
        if let Some(url_pos) = content.to_lowercase().find("url=") {
            let url = content[url_pos + 4..].trim();
            return Some(url.to_string());
        }
    }
    // Also try <meta http-equiv="refresh" content="0; url=...">
    if let Some(pos) = lower.find("http-equiv='refresh'") {
        let start = lower[pos..].find("content='")?;
        let content_start = pos + start + 9;
        let content_end = lower[content_start..].find('\'')?;
        let content = &body[content_start..content_start + content_end];
        if let Some(url_pos) = content.to_lowercase().find("url=") {
            let url = content[url_pos + 4..].trim();
            return Some(url.to_string());
        }
    }
    None
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
