use std::sync::Arc;

use futures::StreamExt;
use httpmock::Method::POST;
use httpmock::MockServer;
use playpen_content::Event;
use playpen_session::Session;
use tokio::sync::Mutex;

use super::{DeepSeekFilesClient, DeepSeekImageUploader, content_hash, resolve_uri};
use crate::convert::ImageUploader;

#[tokio::test]
async fn test_upload_success_returns_file_id() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/files");
        then.status(200)
            .header("content-type", "application/json")
            .body(
                r#"{"id":"file-api-abc123","object":"file","bytes":1024,"created_at":1700000000,"filename":"image.png","purpose":"user_data"}"#,
            );
    });
    let client = DeepSeekFilesClient::new(&server.base_url(), "sk-test");
    let id = client
        .upload("image.png", vec![1, 2, 3], "image/png")
        .await
        .unwrap();
    assert_eq!(id, "file-api-abc123");
}

#[tokio::test]
async fn test_upload_error_surfaces_status_and_body() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST).path("/files");
        then.status(400).body("purpose must be user_data");
    });
    let client = DeepSeekFilesClient::new(&server.base_url(), "sk-test");
    let err = client
        .upload("image.png", vec![1], "image/png")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("HTTP 400"), "{err}");
}

#[tokio::test]
async fn test_upload_sends_multipart_form_with_purpose() {
    let server = MockServer::start();
    server.mock(|when, then| {
        when.method(POST)
            .path("/files")
            .header("authorization", "Bearer sk-test")
            .body_contains("name=\"purpose\"")
            .body_contains("user_data")
            .body_contains("name=\"file\"");
        then.status(200).body(
            r#"{"id":"file-api-x","object":"file","bytes":1,"created_at":1,"filename":"a.png","purpose":"user_data"}"#,
        );
    });
    let client = DeepSeekFilesClient::new(&server.base_url(), "sk-test");
    client.upload("a.png", vec![1], "image/png").await.unwrap();
}

/// 简单的内存 session 桩（记录 StateUpdate 事件，state 恒为空）。
struct StubSession {
    stub_events: StubEvents,
}

impl StubSession {
    fn new() -> Self {
        Self {
            stub_events: StubEvents {
                events: Arc::new(Mutex::new(Vec::new())),
            },
        }
    }
}

impl Session for StubSession {
    fn id(&self) -> &str {
        "stub"
    }
    fn state(&self) -> &dyn playpen_session::State {
        &StubState
    }
    fn events(&self) -> &dyn playpen_session::Events {
        &self.stub_events
    }
}

struct StubState;

#[async_trait::async_trait]
impl playpen_session::State for StubState {
    async fn get(&self, _key: &str) -> Option<serde_json::Value> {
        None
    }
    async fn entities(&self) -> futures::stream::BoxStream<'_, (String, serde_json::Value)> {
        futures::stream::empty().boxed()
    }
}

struct StubEvents {
    events: Arc<Mutex<Vec<Event>>>,
}

#[async_trait::async_trait]
impl playpen_session::Events for StubEvents {
    async fn all(&self) -> futures::stream::BoxStream<'_, Event> {
        futures::stream::empty().boxed()
    }
    async fn len(&self) -> usize {
        0
    }
    async fn append(&self, event: &Event) -> anyhow::Result<Event> {
        self.events.lock().await.push(event.clone());
        Ok(event.clone())
    }
    fn by_role(&self, _roles: &[playpen_session::Role]) -> Box<dyn playpen_session::Events + '_> {
        Box::new(StubEvents {
            events: self.events.clone(),
        })
    }
}

#[tokio::test]
async fn test_uploader_caches_same_content() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/files");
        then.status(200).body(
            r#"{"id":"file-api-cached","object":"file","bytes":1,"created_at":1,"filename":"a.png","purpose":"user_data"}"#,
        );
    });
    let session: Arc<dyn Session> = Arc::new(StubSession::new());
    let uploader = DeepSeekImageUploader::new(
        DeepSeekFilesClient::new(&server.base_url(), "sk-test"),
        ".",
        Some(session),
    );

    let data = b"same-image-bytes".to_vec();
    let id1 = uploader
        .upload_image("a.png", "image/png", data.clone())
        .await
        .unwrap();
    let id2 = uploader
        .upload_image("a.png", "image/png", data)
        .await
        .unwrap();
    assert_eq!(id1, "file-api-cached");
    assert_eq!(id2, "file-api-cached");
    // 同一内容只上传一次（进程内缓存命中）
    assert_eq!(mock.hits(), 1);
}

#[tokio::test]
async fn test_uploader_different_content_uploads_twice() {
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method(POST).path("/files");
        then.status(200).body(
            r#"{"id":"file-api-diff","object":"file","bytes":1,"created_at":1,"filename":"a.png","purpose":"user_data"}"#,
        );
    });
    let uploader = DeepSeekImageUploader::new(
        DeepSeekFilesClient::new(&server.base_url(), "sk-test"),
        ".",
        None,
    );
    uploader
        .upload_image("a.png", "image/png", b"one".to_vec())
        .await
        .unwrap();
    uploader
        .upload_image("a.png", "image/png", b"two".to_vec())
        .await
        .unwrap();
    assert_eq!(mock.hits(), 2);
}

#[test]
fn test_content_hash_deterministic() {
    assert_eq!(content_hash(b"abc"), content_hash(b"abc"));
    assert_ne!(content_hash(b"abc"), content_hash(b"abd"));
}

#[test]
fn test_resolve_uri_variants() {
    use std::path::PathBuf;
    let wd = PathBuf::from("/work");
    assert_eq!(
        resolve_uri("file:///abs/x.png", &wd),
        PathBuf::from("/abs/x.png")
    );
    assert_eq!(resolve_uri("/abs/x.png", &wd), PathBuf::from("/abs/x.png"));
    assert_eq!(resolve_uri("x.png", &wd), PathBuf::from("/work/x.png"));
}
