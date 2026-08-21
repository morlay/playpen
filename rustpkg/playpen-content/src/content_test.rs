use super::*;

#[test]
fn test_format_text_block() {
    let block = ContentBlock::text("hello world");
    assert_eq!(format_content_block(&block), "hello world");
}

#[test]
fn test_format_resource_text_block() {
    let block = ContentBlock::resource(Resource::text(
        "/path/to/main.rs",
        "text/x-rust",
        "fn main() {}",
    ));
    let result = format_content_block(&block);
    assert!(result.starts_with("```rs"));
    assert!(result.contains("{uri=\"/path/to/main.rs\"}"));
    assert!(result.contains("fn main() {}"));
}

#[test]
fn test_format_resource_text_plain() {
    let block = ContentBlock::resource(Resource::text("/path/to/file.txt", "text/plain", "hello"));
    let result = format_content_block(&block);
    assert_eq!(result, "``` {uri=\"/path/to/file.txt\"}\nhello\n```");
}

#[test]
fn test_format_resource_text_empty_type() {
    let block = ContentBlock::resource(Resource::text("/path/to/file", "", "content"));
    let result = format_content_block(&block);
    assert!(result.contains("content"));
}

#[test]
fn test_format_resource_blob_block() {
    let block = ContentBlock::resource(Resource::blob(
        "/path/to/img.png",
        "image/png",
        vec![0x89, 0x50],
    ));
    let result = format_content_block(&block);
    assert!(result.starts_with("```png"));
    assert!(result.contains("{uri=\"/path/to/img.png\" type=\"image/png\" base64}"));
    assert!(result.contains("iVA="));
}

#[test]
fn test_format_resource_blob_empty_type() {
    let block = ContentBlock::resource(Resource::blob("/path/to/file", "", vec![0x00, 0x01]));
    let result = format_content_block(&block);
    assert!(result.contains("{uri=\"/path/to/file\" base64}"));
    assert!(!result.contains("type="));
}

#[test]
fn test_format_resource_link_with_type() {
    let block = ContentBlock::resource_link(
        ResourceLink::new("https://example.com/doc.pdf", "doc").with_media_type("application/pdf"),
    );
    assert_eq!(
        format_content_block(&block),
        "[doc](https://example.com/doc.pdf){type=application/pdf}"
    );
}

#[test]
fn test_format_resource_link_without_type() {
    let block = ContentBlock::resource_link(ResourceLink::new("https://example.com/doc", "doc"));
    assert_eq!(
        format_content_block(&block),
        "[doc](https://example.com/doc)"
    );
}

#[test]
fn test_format_text_block_with_newlines() {
    let block = ContentBlock::text("line1\nline2");
    assert_eq!(format_content_block(&block), "line1\nline2");
}

#[test]
fn test_blob_serde_roundtrip() {
    let blob = Resource::blob("/img.png", "image/png", vec![0x89, 0x50, 0x4e, 0x47]);
    let json = serde_json::to_value(&blob).unwrap();

    // blob 字段应为 base64 字符串，而非数字数组
    assert_eq!(
        json["blob"],
        serde_json::json!("iVBORw=="),
        "blob 应序列化为 base64"
    );

    let decoded: Resource = serde_json::from_value(json).unwrap();
    match decoded {
        Resource::Blob { blob, .. } => {
            assert_eq!(blob, vec![0x89, 0x50, 0x4e, 0x47]);
        }
        _ => panic!("应反序列化为 Blob"),
    }
}

#[test]
fn test_content_block_resource_serde_roundtrip() {
    // 嵌套 enum 的 tag 冲突回归测试：ContentBlock::Resource 序列化后
    // 必须能被反序列化回来（DB 持久化依赖此 roundtrip）。
    let block = ContentBlock::Resource(Resource::Blob {
        uri: "file:///tmp/a.png".into(),
        media_type: "image/png".into(),
        blob: vec![0x89, 0x50, 0x4e, 0x47],
        annotations: None,
    });
    let json = serde_json::to_value(&block).unwrap();
    // 外层 type=resource，内层 resource_type=blob，不得出现重复的 type 键
    assert_eq!(json["type"], "resource");
    assert_eq!(json["resource_type"], "blob");
    assert!(json.get("type2").is_none(), "不应出现重复 type 键");

    let decoded: ContentBlock = serde_json::from_value(json).unwrap();
    match decoded {
        ContentBlock::Resource(Resource::Blob { blob, .. }) => {
            assert_eq!(blob, vec![0x89, 0x50, 0x4e, 0x47]);
        }
        _ => panic!("应反序列化为 Resource::Blob"),
    }
}

#[test]
fn test_content_block_resource_text_roundtrip() {
    let block = ContentBlock::Resource(Resource::text("file:///tmp/a.txt", "text/plain", "hello"));
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "resource");
    assert_eq!(json["resource_type"], "text");

    let decoded: ContentBlock = serde_json::from_value(json).unwrap();
    match decoded {
        ContentBlock::Resource(Resource::Text { text, .. }) => {
            assert_eq!(text, "hello");
        }
        _ => panic!("应反序列化为 Resource::Text"),
    }
}
