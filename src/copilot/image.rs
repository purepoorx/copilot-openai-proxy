use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use reqwest::cookie::Jar;
use tracing::debug;

const ATTACHMENTS_URL: &str = "https://copilot.microsoft.com/c/api/attachments";

use crate::copilot::client::COPILOT_USER_AGENT;
use crate::copilot::cookie::build_cookie_header;

/// Upload an image attachment to Copilot.
/// Returns the attachment ID or URL reference.
pub async fn upload_image(
    http: &reqwest::Client,
    jar: &Arc<Jar>,
    image_data: &[u8],
    filename: &str,
    content_type: &str,
) -> Result<String> {
    let cookie_header = build_cookie_header(jar);

    let part = reqwest::multipart::Part::bytes(image_data.to_vec())
        .file_name(filename.to_string())
        .mime_str(content_type)?;

    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = http
        .post(ATTACHMENTS_URL)
        .header("Cookie", &cookie_header)
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Referer", "https://copilot.microsoft.com/")
        .header("Origin", "https://copilot.microsoft.com")
        .multipart(form)
        .send()
        .await
        .context("upload attachment request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("upload image failed: status {status}, body: {body}");
    }

    let body: serde_json::Value = resp.json::<serde_json::Value>().await.context("decode attachment upload response failed")?;
    debug!("upload response: {body}");

    // Extract attachment ID from response
    let attachment_id = body
        .get("attachment_id")
        .or_else(|| body.get("id"))
        .or_else(|| body.get("attachmentId"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(attachment_id)
}

/// Process an image URL: either decode a data URL or download a remote image.
/// Returns (image_data, filename, content_type).
pub async fn process_image_url(
    http: &reqwest::Client,
    url: &str,
) -> Result<(Vec<u8>, String, String)> {
    if url.starts_with("data:") {
        // Parse data URL: data:[<mediatype>][;base64],<data>
        let rest = url.strip_prefix("data:").unwrap();
        let (meta, data) = rest
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("invalid data URL format"))?;

        let (mime_type, is_base64) = if meta.ends_with(";base64") {
            (meta.strip_suffix(";base64").unwrap_or(meta), true)
        } else {
            (meta, false)
        };

        let image_data = if is_base64 {
            base64::engine::general_purpose::STANDARD
                .decode(data)
                .context("base64 decode failed")?
        } else {
            data.as_bytes().to_vec()
        };

        let ext = mime_to_ext(mime_type);
        let filename = format!("image_1.{ext}");

        Ok((image_data, filename, mime_type.to_string()))
    } else {
        // Download remote image
        let resp = http
            .get(url)
            .send()
            .await
            .context("failed to download image")?;

        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();

        let image_data = resp.bytes().await?.to_vec();
        let ext = mime_to_ext(&content_type);
        let filename = format!("image_1.{ext}");

        Ok((image_data, filename, content_type))
    }
}

/// Map MIME type to file extension
fn mime_to_ext(mime: &str) -> &str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => {
            // Try mime_guess
            mime_guess::get_mime_extensions_str(mime)
                .and_then(|exts| exts.first())
                .unwrap_or(&"bin")
        }
    }
}
