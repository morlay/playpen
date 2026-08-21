//! DeepSeek Files API 客户端与图片上传器。
//!
//! 图片类内容先通过 `POST {base_url}/files` 上传（multipart/form-data），
//! 得到 `file_id` 后在 chat 请求中以 `{"type":"file","file_id":...}` 块引用。
//! 见 <https://api-docs.deepseek.com/guides/files_api>。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use playpen_session::Session;
use tokio::sync::Mutex;

use crate::convert::ImageUploader;

/// DeepSeek Files API 客户端。
pub struct DeepSeekFilesClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

/// `POST /files` 成功响应。
#[derive(Debug, serde::Deserialize)]
struct FileUploadResponse {
    id: String,
    #[allow(dead_code)]
    object: String,
    #[allow(dead_code)]
    bytes: u64,
    #[allow(dead_code)]
    created_at: u64,
    #[allow(dead_code)]
    filename: String,
    #[allow(dead_code)]
    purpose: String,
}

impl DeepSeekFilesClient {
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// 上传一个图片文件，返回 `file_id`（`file-api-...`）。
    ///
    /// `purpose` 固定为 `user_data`；不传 `expires_after`，文件永久保留。
    pub async fn upload(
        &self,
        filename: &str,
        bytes: Vec<u8>,
        media_type: &str,
    ) -> anyhow::Result<String> {
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(media_type)
            .map_err(|e| anyhow::anyhow!("invalid media type {media_type:?}: {e}"))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("purpose", "user_data");

        let resp = self
            .http
            .post(format!("{}/files", self.base_url))
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("DeepSeek files upload request failed: {e}"))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("DeepSeek files upload: read response failed: {e}"))?;
        if !status.is_success() {
            anyhow::bail!("DeepSeek files upload failed: HTTP {status}: {body}");
        }

        let parsed: FileUploadResponse = serde_json::from_str(&body)
            .map_err(|e| anyhow::anyhow!("DeepSeek files upload: invalid response: {e}: {body}"))?;
        Ok(parsed.id)
    }
}

/// 生产图片上传器：DeepSeek Files API + 去重缓存（进程内 + session 持久化）。
///
/// 缓存键为图片内容确定性 hash（uuid v5），同一图片在同一 session 内
/// 只上传一次；`resume` 后通过 session 的 `StateUpdate` 事件复用 `file_id`。
pub struct DeepSeekImageUploader {
    client: DeepSeekFilesClient,
    working_dir: PathBuf,
    /// 内容 hash → file_id（进程内缓存）
    cache: Arc<Mutex<HashMap<String, String>>>,
    /// 可选：session 持久化（state 读取 + events 写入）
    session: Option<Arc<dyn Session>>,
}

impl DeepSeekImageUploader {
    pub fn new(
        client: DeepSeekFilesClient,
        working_dir: impl Into<PathBuf>,
        session: Option<Arc<dyn Session>>,
    ) -> Self {
        Self {
            client,
            working_dir: working_dir.into(),
            cache: Arc::new(Mutex::new(HashMap::new())),
            session,
        }
    }
}

/// 内容确定性 hash（sha256，用于去重缓存 key）。
fn content_hash(data: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(data);
    format!("{digest:x}")
}

fn cache_key(hash: &str) -> String {
    format!("ds-file:{hash}")
}

/// 解析资源 uri 为本地路径（支持 `file://`、绝对路径、相对 working_dir 路径）。
fn resolve_uri(uri: &str, working_dir: &Path) -> PathBuf {
    if let Some(path) = uri.strip_prefix("file://") {
        return PathBuf::from(path);
    }
    let p = Path::new(uri);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        working_dir.join(p)
    }
}

#[async_trait::async_trait]
impl ImageUploader for DeepSeekImageUploader {
    async fn upload_image(
        &self,
        name: &str,
        media_type: &str,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        let hash = content_hash(&data);
        let key = cache_key(&hash);

        // 1. 进程内缓存
        if let Some(file_id) = self.cache.lock().await.get(&hash).cloned() {
            return Ok(file_id);
        }

        // 2. session 持久化缓存（resume 复用）
        if let Some(session) = &self.session
            && let Some(value) = session.state().get(&key).await
            && let Some(file_id) = value.as_str()
        {
            self.cache
                .lock()
                .await
                .insert(hash.clone(), file_id.to_string());
            return Ok(file_id.to_string());
        }

        // 3. 上传
        let file_id = self.client.upload(name, data, media_type).await?;

        // 4. 写缓存
        self.cache
            .lock()
            .await
            .insert(hash.clone(), file_id.clone());
        if let Some(session) = &self.session
            && let Err(e) = session
                .events()
                .append(&playpen_content::Event::StateUpdate {
                    id: String::new(),
                    name: key,
                    data: serde_json::json!(file_id),
                })
                .await
        {
            tracing::warn!(error = %e, "persist ds-file cache failed");
        }

        Ok(file_id)
    }

    async fn read_image_file(&self, uri: &str) -> anyhow::Result<Vec<u8>> {
        let path = resolve_uri(uri, &self.working_dir);
        let data = tokio::fs::read(&path)
            .await
            .map_err(|e| anyhow::anyhow!("read image file {} failed: {e}", path.display()))?;
        Ok(data)
    }
}

/// 便捷构造：从 base_url/api_key 构建上传器。
impl DeepSeekImageUploader {
    pub fn from_parts(
        base_url: &str,
        api_key: &str,
        working_dir: impl Into<PathBuf>,
        session: Option<Arc<dyn Session>>,
    ) -> Self {
        Self::new(
            DeepSeekFilesClient::new(base_url, api_key),
            working_dir,
            session,
        )
    }
}

#[cfg(test)]
#[path = "files_test.rs"]
mod tests;
