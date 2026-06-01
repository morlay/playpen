use std::time::Duration;

use crate::fetch::{FetchError, FetchOption, FetchResult, Fetcher};

pub struct NativeFetcher;

impl Fetcher for NativeFetcher {
    fn fetch(&self, opt: FetchOption) -> anyhow::Result<FetchResult> {
        let timeout = Duration::from_millis(opt.timeout_ms.unwrap_or(30000));
        let max_bytes = opt.max_bytes.unwrap_or(10 * 1024 * 1024);

        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .user_agent("playpen-agent/1.0")
            .build()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let response = client
            .get(&opt.url)
            .send()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::HttpStatus(format!("HTTP {}", status.as_u16())).into());
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response
            .bytes()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let bytes = if bytes.len() > max_bytes {
            bytes.slice(..max_bytes)
        } else {
            bytes
        };

        let is_html = content_type.starts_with("text/html");
        let want_html = opt
            .accept
            .as_deref()
            .is_none_or(|ct| ct.is_empty() || ct == "text/html");

        if is_html && want_html {
            let html = String::from_utf8_lossy(&bytes).to_string();
            let md = html2md::parse_html(&html);
            Ok(FetchResult {
                content: md,
                media_type: "text/markdown".into(),
            })
        } else if content_type.contains("charset") || content_type.starts_with("text/") {
            Ok(FetchResult {
                content: String::from_utf8_lossy(&bytes).to_string(),
                media_type: content_type,
            })
        } else {
            Ok(FetchResult {
                content: format!(
                    "[非文本内容，Content-Type: {content_type}，{} 字节]",
                    bytes.len()
                ),
                media_type: content_type,
            })
        }
    }
}

#[cfg(test)]
#[path = "native_fetch_test.rs"]
mod native_fetch_test;
