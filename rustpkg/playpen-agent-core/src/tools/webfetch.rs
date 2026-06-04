use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebfetchParams {
    #[schemars(description = "要获取的 URL")]
    pub url: String,
    #[schemars(description = "超时时间（毫秒，可选，默认 30000）")]
    pub timeout_ms: Option<u64>,
    #[schemars(description = "最大响应字节数（可选，默认 10485760）")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, thiserror::Error)]
pub enum WebfetchError {
    #[error("HTTP 请求失败：{0}")]
    Http(String),
    #[error("读取响应失败：{0}")]
    Io(#[from] std::io::Error),
}

// ── rig Tool ──

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;

pub struct WebfetchRigTool;

impl Tool for WebfetchRigTool {
    const NAME: &'static str = "webfetch";
    type Error = WebfetchError;
    type Args = WebfetchParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(WebfetchParams)).unwrap();
        ToolDefinition {
            name: "webfetch".into(),
            description: "获取网页内容".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let timeout = std::time::Duration::from_millis(args.timeout_ms.unwrap_or(30000));
        let max_bytes = args.max_bytes.unwrap_or(10 * 1024 * 1024);

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("playpen-agent/1.0")
            .build()
            .map_err(|e| WebfetchError::Http(e.to_string()))?;

        let response = client
            .get(&args.url)
            .send()
            .await
            .map_err(|e| WebfetchError::Http(e.to_string()))?;

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let status = response.status();
        if !status.is_success() {
            return Err(WebfetchError::Http(format!("HTTP {}", status.as_u16())));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| WebfetchError::Http(e.to_string()))?;

        let bytes = if bytes.len() > max_bytes { bytes.slice(..max_bytes) } else { bytes };
        let text = if content_type.contains("charset") || content_type.starts_with("text/") {
            String::from_utf8_lossy(&bytes).to_string()
        } else {
            format!("[非文本内容，Content-Type: {}，{} 字节]", content_type, bytes.len())
        };
        Ok(text)
    }
}
