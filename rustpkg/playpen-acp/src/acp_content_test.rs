use agent_client_protocol::schema::v1::{self, Annotations};
use agent_client_protocol::schema::v1::{
    BlobResourceContents, ContentBlock as AcpContentBlock, EmbeddedResource,
    EmbeddedResourceResource, ResourceLink as AcpResourceLink, TextContent as AcpTextContent,
    TextResourceContents,
};
use playpen_content::{ContentBlock, Resource, ResourceLink, TextContent};

use crate::acp_content::{
    collect_acp_annotations, extract_acp_annotations, to_acp_blocks, to_agent_blocks,
};

/// 辅助：构建包含 acp.title / acp.description 的 annotations。
fn ann_with_title_desc(title: &str, desc: &str) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("acp.title".into(), title.into());
    map.insert("acp.description".into(), desc.into());
    Some(serde_json::Value::Object(map))
}

/// 辅助：合并已有的 annotations 与 title/description。
fn merge_title_desc(
    ann: Option<serde_json::Value>,
    title: &str,
    desc: &str,
) -> Option<serde_json::Value> {
    let mut map = ann.and_then(|v| v.as_object().cloned()).unwrap_or_default();
    map.insert("acp.title".into(), title.into());
    map.insert("acp.description".into(), desc.into());
    Some(serde_json::Value::Object(map))
}

// ── to_agent_blocks ───────────────────────────────────────────────────

#[test]
fn test_to_agent_blocks_text() {
    let acp = vec![AcpContentBlock::Text(AcpTextContent::new("hello"))];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Text(t) => {
            assert_eq!(t.text, "hello");
            assert!(
                t.annotations.is_none(),
                "无 annotations/meta 时不产生 annotations"
            );
        }
        other => panic!("期望 Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_text_with_annotations_and_meta() {
    let acp = vec![AcpContentBlock::Text(
        AcpTextContent::new("hi")
            .annotations(
                Annotations::new()
                    .audience(vec![v1::Role::User])
                    .last_modified("2024-01-01"),
            )
            .meta(v1::Meta::from_iter([("origin".into(), "test".into())])),
    )];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Text(t) => {
            let ann = t.annotations.as_ref().expect("应有 annotations");
            let obj = ann.as_object().expect("annotations 应是 object");
            assert!(obj.contains_key("acp.annotations"), "应有 acp.annotations");
            // meta 拍平为 _meta.* 前缀
            assert_eq!(
                obj.get("_meta.origin").and_then(|v| v.as_str()),
                Some("test"),
                "meta 应拍平为 _meta.origin"
            );
            // 回读验证
            let (ann_back, meta_back) = extract_acp_annotations(&t.annotations);
            assert!(ann_back.is_some(), "acp.annotations 可回读");
            assert!(meta_back.is_some(), "_meta.* 可回读");
            if let Some(m) = meta_back {
                assert_eq!(m.get("origin"), Some(&"test".into()));
            }
        }
        other => panic!("期望 Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_resource_link() {
    let acp = vec![AcpContentBlock::ResourceLink(
        AcpResourceLink::new("my-name", "file:///path/to/file.md")
            .description(Some("a file".to_string()))
            .size(Some(1024))
            .title(Some("我的文件".to_string()))
            .annotations(Annotations::new().audience(vec![v1::Role::User]))
            .meta(v1::Meta::from_iter([("source".into(), "test".into())])),
    )];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::ResourceLink(link) => {
            assert_eq!(link.name, "my-name", "name 应来自 acp_link.name");
            assert_eq!(link.uri, "file:///path/to/file.md");
            assert_eq!(link.size, Some(1024), "size 应映射");
            // title / description 应收拢到 annotations
            let ann = link.annotations.as_ref().expect("应有 annotations");
            let obj = ann.as_object().expect("annotations 应是 object");
            assert_eq!(
                obj.get("acp.title").and_then(|v| v.as_str()),
                Some("我的文件"),
                "acp.title 应存在"
            );
            assert_eq!(
                obj.get("acp.description").and_then(|v| v.as_str()),
                Some("a file"),
                "acp.description 应存在"
            );
            assert!(
                obj.contains_key("acp.annotations"),
                "acp.annotations 应存在"
            );
            assert_eq!(
                obj.get("_meta.source").and_then(|v| v.as_str()),
                Some("test"),
                "meta 应拍平为 _meta.source"
            );
        }
        other => panic!("期望 ResourceLink，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_resource_text() {
    let acp = vec![AcpContentBlock::Resource(EmbeddedResource::new(
        EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new("content", "uri://text")
                .mime_type(Some("text/markdown".to_string())),
        ),
    ))];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Resource(Resource::Text {
            uri,
            text,
            media_type,
            ..
        }) => {
            assert_eq!(uri, "uri://text");
            assert_eq!(text, "content");
            assert_eq!(media_type, "text/markdown");
        }
        other => panic!("期望 Resource::Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_resource_blob() {
    let acp = vec![AcpContentBlock::Resource(EmbeddedResource::new(
        EmbeddedResourceResource::BlobResourceContents(
            BlobResourceContents::new("aGVsbG8=", "uri://blob")
                .mime_type(Some("image/png".to_string())),
        ),
    ))];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Resource(Resource::Blob {
            uri,
            blob,
            media_type,
            ..
        }) => {
            assert_eq!(uri, "uri://blob");
            assert_eq!(blob, b"hello");
            assert_eq!(media_type, "image/png");
        }
        other => panic!("期望 Resource::Blob，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_unknown_variant_skipped() {
    let result = to_agent_blocks(&[]);
    assert!(result.is_empty());
}

// ── to_acp_blocks ─────────────────────────────────────────────────────

#[test]
fn test_to_acp_blocks_text() {
    let agent = vec![ContentBlock::Text(TextContent::new("world"))];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Text(t) => {
            assert_eq!(t.text, "world");
            assert!(t.annotations.is_none(), "无 acp.* 时不产生 annotations");
        }
        other => panic!("期望 Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_text_with_acp_annotations_in_annotations() {
    let agent = vec![ContentBlock::Text(playpen_content::TextContent {
        text: "with-ann".to_string(),
        annotations: collect_acp_annotations(
            &Some(
                Annotations::new()
                    .audience(vec![v1::Role::Assistant])
                    .last_modified("2025-06-26"),
            ),
            &Some(v1::Meta::from_iter([("key".into(), "val".into())])),
        ),
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Text(t) => {
            assert_eq!(t.text, "with-ann");
            assert!(t.annotations.is_some(), "ACP Text 应有 annotations");
            assert!(t.meta.is_some(), "ACP Text 应有 meta");
            if let Some(m) = &t.meta {
                assert_eq!(m.get("key"), Some(&"val".into()));
            }
        }
        other => panic!("期望 Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_resource_link() {
    let agent = vec![ContentBlock::ResourceLink(ResourceLink {
        uri: "file:///path/to/doc.md".to_string(),
        name: "my-doc".to_string(),
        media_type: Some("text/plain".to_string()),
        size: Some(2048),
        // title/description 存在 annotations 中
        annotations: merge_title_desc(
            collect_acp_annotations(
                &Some(Annotations::new().audience(vec![v1::Role::User])),
                &Some(v1::Meta::from_iter([("src".into(), "cli".into())])),
            ),
            "标题",
            "a document",
        ),
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::ResourceLink(link) => {
            assert_eq!(link.name, "my-doc", "name 应透传");
            assert_eq!(link.uri, "file:///path/to/doc.md");
            assert_eq!(
                link.description.as_deref(),
                Some("a document"),
                "description 应从 annotations 恢复"
            );
            assert_eq!(link.mime_type.as_deref(), Some("text/plain"));
            assert_eq!(link.size, Some(2048), "size 应回写");
            assert_eq!(
                link.title.as_deref(),
                Some("标题"),
                "title 应从 annotations 恢复"
            );
            assert!(link.annotations.is_some(), "annotations 应回写");
            assert!(link.meta.is_some(), "meta 应回写");
            if let Some(m) = &link.meta {
                assert_eq!(m.get("src"), Some(&"cli".into()));
            }
        }
        other => panic!("期望 ResourceLink，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_resource_link_name_derived_from_path() {
    // 当 name 看起来像文件路径时，to_acp_blocks 应从中推断友好名称
    let agent = vec![ContentBlock::ResourceLink(ResourceLink {
        uri: "file:///skills/code/SKILL.md".to_string(),
        name: "file:///skills/code/SKILL.md".to_string(),
        media_type: None,
        size: None,
        annotations: ann_with_title_desc("", "skill code 的描述"),
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::ResourceLink(link) => {
            assert_eq!(link.name, "skill:code");
            assert_eq!(link.uri, "file:///skills/code/SKILL.md");
        }
        other => panic!("期望 ResourceLink，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_resource_text() {
    let agent = vec![ContentBlock::Resource(Resource::Text {
        uri: "uri://doc".to_string(),
        media_type: "text/plain".to_string(),
        text: "some text".to_string(),
        annotations: None,
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Resource(EmbeddedResource {
            resource:
                EmbeddedResourceResource::TextResourceContents(TextResourceContents {
                    text,
                    uri,
                    mime_type: media_type,
                    ..
                }),
            ..
        }) => {
            assert_eq!(text, "some text");
            assert_eq!(uri, "uri://doc");
            assert_eq!(media_type.as_deref(), Some("text/plain"));
        }
        other => panic!("期望 Resource::TextResourceContents，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_resource_blob() {
    let agent = vec![ContentBlock::Resource(Resource::Blob {
        uri: "uri://img".to_string(),
        media_type: "image/png".to_string(),
        blob: b"\x89PNG\r\n\x1a\n".to_vec(),
        annotations: None,
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Resource(EmbeddedResource {
            resource:
                EmbeddedResourceResource::BlobResourceContents(BlobResourceContents {
                    blob,
                    uri,
                    mime_type: media_type,
                    ..
                }),
            ..
        }) => {
            assert_eq!(uri, "uri://img");
            assert_eq!(blob, "iVBORw0KGgo=", "blob 应编码为 base64");
            assert_eq!(media_type.as_deref(), Some("image/png"));
        }
        other => panic!("期望 Resource::BlobResourceContents，得到 {other:?}"),
    }
}

// ── to_acp_blocks: Resource annotations ────────────────────────────────

#[test]
fn test_to_acp_blocks_resource_text_with_annotations() {
    let agent = vec![ContentBlock::Resource(Resource::Text {
        uri: "uri://doc".to_string(),
        media_type: "text/plain".to_string(),
        text: "some text".to_string(),
        annotations: Some(serde_json::json!({"audience": ["user"], "priority": 0.5})),
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Resource(EmbeddedResource {
            resource:
                EmbeddedResourceResource::TextResourceContents(TextResourceContents {
                    text, uri, ..
                }),
            annotations: acp_ann,
            ..
        }) => {
            assert_eq!(text, "some text");
            assert_eq!(uri, "uri://doc");
            assert!(acp_ann.is_some(), "annotations 应透传");
        }
        other => panic!("期望 Resource::TextResourceContents，得到 {other:?}"),
    }
}

#[test]
fn test_to_acp_blocks_resource_blob_with_annotations() {
    let agent = vec![ContentBlock::Resource(Resource::Blob {
        uri: "uri://img".to_string(),
        media_type: "image/png".to_string(),
        blob: b"\x89PNG".to_vec(),
        annotations: Some(serde_json::json!({"audience": ["user"]})),
    })];
    let result = to_acp_blocks(&agent);
    assert_eq!(result.len(), 1);
    match &result[0] {
        AcpContentBlock::Resource(EmbeddedResource {
            resource:
                EmbeddedResourceResource::BlobResourceContents(BlobResourceContents { uri, .. }),
            annotations: acp_ann,
            ..
        }) => {
            assert_eq!(uri, "uri://img");
            assert!(acp_ann.is_some(), "annotations 应透传");
        }
        other => panic!("期望 Resource::BlobResourceContents，得到 {other:?}"),
    }
}

// ── to_agent_blocks: Resource annotations ──────────────────────────────

#[test]
fn test_to_agent_blocks_resource_text_with_annotations() {
    let acp = vec![AcpContentBlock::Resource(
        EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
            TextResourceContents::new("content", "uri://text")
                .mime_type(Some("text/markdown".to_string())),
        ))
        .annotations(Annotations::new().audience(vec![v1::Role::User])),
    )];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Resource(Resource::Text {
            uri,
            text,
            annotations,
            ..
        }) => {
            assert_eq!(uri, "uri://text");
            assert_eq!(text, "content");
            assert!(annotations.is_some(), "annotations 应透传");
        }
        other => panic!("期望 Resource::Text，得到 {other:?}"),
    }
}

#[test]
fn test_to_agent_blocks_resource_blob_with_annotations() {
    let acp = vec![AcpContentBlock::Resource(
        EmbeddedResource::new(EmbeddedResourceResource::BlobResourceContents(
            BlobResourceContents::new("aGVsbG8=", "uri://blob")
                .mime_type(Some("image/png".to_string())),
        ))
        .annotations(Annotations::new().audience(vec![v1::Role::User])),
    )];
    let result = to_agent_blocks(&acp);
    assert_eq!(result.len(), 1);
    match &result[0] {
        ContentBlock::Resource(Resource::Blob {
            uri,
            blob,
            annotations,
            ..
        }) => {
            assert_eq!(uri, "uri://blob");
            assert_eq!(blob, b"hello");
            assert!(annotations.is_some(), "annotations 应透传");
        }
        other => panic!("期望 Resource::Blob，得到 {other:?}"),
    }
}

// ── round-trip ────────────────────────────────────────────────────────

#[test]
fn test_round_trip_resource_text() {
    let original = vec![ContentBlock::Resource(Resource::Text {
        uri: "uri://doc".to_string(),
        media_type: "text/plain".to_string(),
        text: "some text".to_string(),
        annotations: None,
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (
            ContentBlock::Resource(Resource::Text {
                uri,
                text,
                media_type,
                ..
            }),
            ContentBlock::Resource(Resource::Text {
                uri: u2,
                text: t2,
                media_type: m2,
                ..
            }),
        ) => {
            assert_eq!(uri, u2);
            assert_eq!(text, t2);
            assert_eq!(media_type, m2);
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_resource_text_with_annotations() {
    let original = vec![ContentBlock::Resource(Resource::Text {
        uri: "uri://doc".to_string(),
        media_type: "text/plain".to_string(),
        text: "some text".to_string(),
        annotations: Some(serde_json::json!({"audience": ["user"], "priority": 0.5})),
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (
            ContentBlock::Resource(Resource::Text { annotations: a, .. }),
            ContentBlock::Resource(Resource::Text { annotations: b, .. }),
        ) => {
            assert_eq!(a, b, "annotations round-trip");
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_resource_blob() {
    let original = vec![ContentBlock::Resource(Resource::Blob {
        uri: "uri://img".to_string(),
        media_type: "image/png".to_string(),
        blob: b"\x89PNG".to_vec(),
        annotations: None,
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (
            ContentBlock::Resource(Resource::Blob {
                uri,
                media_type,
                blob,
                ..
            }),
            ContentBlock::Resource(Resource::Blob {
                uri: u2,
                media_type: m2,
                blob: b2,
                ..
            }),
        ) => {
            assert_eq!(uri, u2);
            assert_eq!(media_type, m2);
            assert_eq!(blob, b2);
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_text() {
    let original = vec![ContentBlock::Text(TextContent::new("round trip"))];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (ContentBlock::Text(a), ContentBlock::Text(b)) => {
            assert_eq!(a.text, b.text);
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_text_with_acp_annotations() {
    let original = vec![ContentBlock::Text(playpen_content::TextContent {
        text: "ann-text".to_string(),
        annotations: collect_acp_annotations(
            &Some(
                Annotations::new()
                    .audience(vec![v1::Role::User])
                    .last_modified("2025-01-01"),
            ),
            &Some(v1::Meta::from_iter([("k".into(), "v".into())])),
        ),
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (ContentBlock::Text(a), ContentBlock::Text(b)) => {
            assert_eq!(a.text, b.text);
            assert_eq!(a.annotations, b.annotations, "annotations round-trip");
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_resource_link() {
    let original = vec![ContentBlock::ResourceLink(ResourceLink {
        uri: "file:///path/to/resource".to_string(),
        name: "resource-name".to_string(),
        media_type: None,
        size: None,
        annotations: ann_with_title_desc("", "a resource"),
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (ContentBlock::ResourceLink(a), ContentBlock::ResourceLink(b)) => {
            assert_eq!(a.name, b.name, "name round-trip");
            assert_eq!(a.uri, b.uri, "uri round-trip");
            assert_eq!(a.annotations, b.annotations, "annotations round-trip");
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}

#[test]
fn test_round_trip_resource_link_full() {
    let original = vec![ContentBlock::ResourceLink(ResourceLink {
        uri: "file:///path/to/resource".to_string(),
        name: "full-resource".to_string(),
        media_type: Some("text/markdown".to_string()),
        size: Some(4096),
        annotations: merge_title_desc(
            collect_acp_annotations(
                &Some(
                    Annotations::new()
                        .audience(vec![v1::Role::User, v1::Role::Assistant])
                        .priority(0.8),
                ),
                &Some(v1::Meta::from_iter([("origin".into(), "cli".into())])),
            ),
            "完整资源",
            "a full resource",
        ),
    })];
    let acp = to_acp_blocks(&original);
    let back = to_agent_blocks(&acp);
    assert_eq!(original.len(), back.len());
    match (&original[0], &back[0]) {
        (ContentBlock::ResourceLink(a), ContentBlock::ResourceLink(b)) => {
            assert_eq!(a.name, b.name, "name round-trip");
            assert_eq!(a.uri, b.uri, "uri round-trip");
            assert_eq!(a.media_type, b.media_type, "media_type round-trip");
            assert_eq!(a.size, b.size, "size round-trip");
            assert_eq!(a.annotations, b.annotations, "annotations round-trip");
        }
        _ => panic!("round-trip 后类型不匹配"),
    }
}
