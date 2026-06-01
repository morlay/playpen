use super::*;
use crate::fetch::{FetchOption, Fetcher};

#[test]
fn fetch_text() {
    let server = httpmock::MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/data");
        then.status(200)
            .header("Content-Type", "text/plain")
            .body("hello from web");
    });

    let fetcher = NativeFetcher;
    let result = fetcher
        .fetch(FetchOption {
            url: server.url("/data"),
            timeout_ms: None,
            max_bytes: None,
            accept: None,
        })
        .unwrap();

    assert_eq!(result.content, "hello from web");
    mock.assert_hits(1);
}

#[test]
fn fetch_not_found() {
    let server = httpmock::MockServer::start();
    server.mock(|when, then| {
        when.method("GET").path("/missing");
        then.status(404);
    });

    let fetcher = NativeFetcher;
    let result = fetcher.fetch(FetchOption {
        url: server.url("/missing"),
        timeout_ms: None,
        max_bytes: None,
        accept: None,
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.downcast_ref::<crate::fetch::FetchError>().is_some());
}

#[test]
fn fetch_html_to_markdown() {
    let server = httpmock::MockServer::start();
    server.mock(|when, then| {
        when.method("GET").path("/page");
        then.status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body("<html><body><h1>Title</h1><p>Hello <strong>World</strong></p></body></html>");
    });

    let fetcher = NativeFetcher;
    let result = fetcher
        .fetch(FetchOption {
            url: server.url("/page"),
            timeout_ms: None,
            max_bytes: None,
            accept: None,
        })
        .unwrap();

    assert!(
        result.content.contains("Title"),
        "应包含标题: {}",
        result.content
    );
    assert!(
        result.content.contains("Hello"),
        "应包含段落: {}",
        result.content
    );
}

#[test]
fn fetch_invalid_url() {
    let fetcher = NativeFetcher;
    let result = fetcher.fetch(FetchOption {
        url: "http://invalid.invalid/.invalid".into(),
        timeout_ms: Some(1000),
        max_bytes: None,
        accept: None,
    });
    assert!(result.is_err());
}
