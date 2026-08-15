use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FetchOption {
    pub url: String,
    pub timeout_ms: Option<u64>,
    pub max_bytes: Option<usize>,
    pub accept: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchResult {
    pub content: String,
    pub media_type: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    HttpStatus(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Network(String),
    #[error("{0}")]
    Parse(String),
}

/// 网络抓取抽象。
///
/// `fetch` 返回 [`anyhow::Result`]，实现可能构造具体的 [`FetchError`]
/// 并通过 `.into()` 传播。调用方可用 `downcast_ref::<FetchError>()`
/// 识别具体错误模式：
///
/// - [`FetchError::HttpStatus`]
/// - [`FetchError::Timeout`]
/// - [`FetchError::Network`]
/// - [`FetchError::Parse`]
pub trait Fetcher: Send + Sync {
    fn fetch(&self, opt: FetchOption) -> anyhow::Result<FetchResult>;
}

#[cfg(test)]
#[path = "fetch_test.rs"]
mod fetch_test;
