use agent_client_protocol::schema::v1::{
    Annotations, BlobResourceContents, ContentBlock as AcpContentBlock, Cost as AcpCost,
    EmbeddedResource, EmbeddedResourceResource, Meta, ResourceLink as AcpResourceLink,
    SessionInfoUpdate, SessionUpdate, TextContent as AcpTextContent, TextResourceContents,
    ToolCall, ToolCallStatus, UsageUpdate,
};
use playpen_config::model::Model;
use playpen_content::{ContentBlock, Resource, ResourceLink, StopReason, TextContent, TokenUsage};

fn text_from_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

pub fn text_from_opt_content(content: &Option<Vec<ContentBlock>>) -> String {
    content
        .as_ref()
        .map(|b| text_from_blocks(b))
        .unwrap_or_default()
}

// ── annotations 转换辅助 ─────────────────────────────────────────────

/// 将 ACP 侧的 annotations + meta 收拢到 agent 侧的 `annotations`（serde_json::Value）。
///
/// - `Annotations` 结构体序列化为 `acp.annotations`
/// - `Meta` 拍平为 `_meta.{key}` 避免多一层嵌套
fn collect_acp_annotations(
    acp_annotations: &Option<Annotations>,
    acp_meta: &Option<Meta>,
) -> Option<serde_json::Value> {
    let mut map = serde_json::Map::new();
    if let Some(ann) = acp_annotations
        && let Ok(val) = serde_json::to_value(ann)
    {
        map.insert("acp.annotations".into(), val);
    }
    if let Some(m) = acp_meta {
        for (k, v) in m {
            map.insert(format!("_meta.{k}"), v.clone());
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

/// 从 agent 侧的 `annotations` 中提取 `acp.annotations` 和拍平的 `_meta.*`。
fn extract_acp_annotations(
    agent_ann: &Option<serde_json::Value>,
) -> (Option<Annotations>, Option<Meta>) {
    let mut acp_ann = None;
    let mut acp_meta = None;
    if let Some(ann_value) = agent_ann
        && let Some(obj) = ann_value.as_object()
    {
        if let Some(val) = obj.get("acp.annotations") {
            acp_ann = serde_json::from_value(val.clone()).ok();
        }
        // 收集所有 _meta. 前缀的 key，还原为 Meta map
        let mut meta = Meta::new();
        for (k, v) in obj {
            if let Some(suffix) = k.strip_prefix("_meta.") {
                meta.insert(suffix.to_string(), v.clone());
            }
        }
        if !meta.is_empty() {
            acp_meta = Some(meta);
        }
    }
    (acp_ann, acp_meta)
}

// ── 块转换 ─────────────────────────────────────────────────────────────

/// 将 `playpen_content::ContentBlock` 序列转换为 `AcpContentBlock` 序列。
pub fn to_acp_blocks(blocks: &[ContentBlock]) -> Vec<AcpContentBlock> {
    use base64::Engine;

    blocks
        .iter()
        .flat_map(|b| match b {
            ContentBlock::Text(t) => {
                let mut tc = AcpTextContent::new(t.text.clone());
                let (ann, meta) = extract_acp_annotations(&t.annotations);
                if let Some(ann) = ann {
                    tc = tc.annotations(ann);
                }
                if let Some(meta) = meta {
                    tc = tc.meta(meta);
                }
                vec![AcpContentBlock::Text(tc)]
            }
            ContentBlock::Resource(r) => match r {
                Resource::Text {
                    uri,
                    text,
                    media_type,
                    annotations,
                } => {
                    let mut embedded =
                        EmbeddedResource::new(EmbeddedResourceResource::TextResourceContents(
                            TextResourceContents::new(text.clone(), uri.clone())
                                .mime_type(Some(media_type.clone())),
                        ));
                    if let Some(ann) = annotations.as_ref()
                        && let Ok(acp_ann) = serde_json::from_value::<Annotations>(ann.clone())
                    {
                        embedded = embedded.annotations(acp_ann);
                    }
                    vec![AcpContentBlock::Resource(embedded)]
                }
                Resource::Blob {
                    uri,
                    blob,
                    media_type,
                    annotations,
                } => {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(blob);
                    let mut embedded =
                        EmbeddedResource::new(EmbeddedResourceResource::BlobResourceContents(
                            BlobResourceContents::new(encoded, uri.clone())
                                .mime_type(Some(media_type.clone())),
                        ));
                    if let Some(ann) = annotations.as_ref()
                        && let Ok(acp_ann) = serde_json::from_value::<Annotations>(ann.clone())
                    {
                        embedded = embedded.annotations(acp_ann);
                    }
                    vec![AcpContentBlock::Resource(embedded)]
                }
            },
            ContentBlock::ResourceLink(link) => {
                // 优先使用 link.name；只有当 name 是文件路径/URI 时才从 uri 推断
                let name = if link.name.contains('/')
                    || link.name.contains('\\')
                    || link.name.starts_with("file://")
                {
                    let path = std::path::Path::new(&link.uri);
                    path.file_name()
                        .map(|n| {
                            let name = n.to_string_lossy();
                            if name == "SKILL.md" {
                                path.parent()
                                    .and_then(|p| p.file_name())
                                    .map(|skill_name| {
                                        format!("skill:{}", skill_name.to_string_lossy())
                                    })
                                    .unwrap_or_else(|| name.to_string())
                            } else {
                                name.to_string()
                            }
                        })
                        .unwrap_or_else(|| link.uri.clone())
                } else {
                    link.name.clone()
                };

                let mut acp_link =
                    AcpResourceLink::new(name, link.uri.clone()).mime_type(link.media_type.clone());
                if let Some(size) = link.size {
                    acp_link = acp_link.size(size as i64);
                }

                // 从 annotations 恢复 acp.title / acp.description（不删除，保持 annotations 完整）
                if let Some(ann_obj) = link.annotations.as_ref().and_then(|v| v.as_object()) {
                    if let Some(title) = ann_obj.get("acp.title").and_then(|v| v.as_str()) {
                        acp_link = acp_link.title(title.to_string());
                    }
                    if let Some(desc) = ann_obj.get("acp.description").and_then(|v| v.as_str()) {
                        acp_link = acp_link.description(desc.to_string());
                    }
                }

                let (ann, meta) = extract_acp_annotations(&link.annotations);
                if let Some(ann) = ann {
                    acp_link = acp_link.annotations(ann);
                }
                if let Some(meta) = meta {
                    acp_link = acp_link.meta(meta);
                }
                vec![AcpContentBlock::ResourceLink(acp_link)]
            }
        })
        .collect()
}

/// 将 `AcpContentBlock` 序列转换为 `playpen_content::ContentBlock` 序列。
pub fn to_agent_blocks(blocks: &[AcpContentBlock]) -> Vec<ContentBlock> {
    use base64::Engine;

    blocks
        .iter()
        .flat_map(|b| match b {
            AcpContentBlock::Text(tc) => {
                vec![ContentBlock::Text(TextContent {
                    text: tc.text.clone(),
                    annotations: collect_acp_annotations(&tc.annotations, &tc.meta),
                })]
            }
            AcpContentBlock::Resource(EmbeddedResource {
                resource:
                    EmbeddedResourceResource::TextResourceContents(TextResourceContents {
                        text,
                        uri,
                        mime_type: media_type,
                        ..
                    }),
                annotations,
                ..
            }) => {
                let agent_ann = annotations
                    .as_ref()
                    .and_then(|a| serde_json::to_value(a).ok());
                vec![ContentBlock::Resource(Resource::Text {
                    uri: uri.clone(),
                    media_type: media_type
                        .clone()
                        .unwrap_or_else(|| "text/plain".to_string()),
                    annotations: agent_ann,
                    text: text.clone(),
                })]
            }
            AcpContentBlock::Resource(EmbeddedResource {
                resource:
                    EmbeddedResourceResource::BlobResourceContents(BlobResourceContents {
                        blob,
                        uri,
                        mime_type: media_type,
                        ..
                    }),
                annotations,
                ..
            }) => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(blob.as_bytes())
                    .unwrap_or_default();
                let agent_ann = annotations
                    .as_ref()
                    .and_then(|a| serde_json::to_value(a).ok());
                vec![ContentBlock::Resource(Resource::Blob {
                    uri: uri.clone(),
                    media_type: media_type
                        .clone()
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    blob: decoded,
                    annotations: agent_ann,
                })]
            }
            AcpContentBlock::ResourceLink(acp_link) => {
                // 合并 acp.annotations / acp.meta / acp.title / acp.description 到 annotations
                let mut ann = collect_acp_annotations(&acp_link.annotations, &acp_link.meta)
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default();
                if let Some(title) = &acp_link.title {
                    ann.insert("acp.title".into(), title.clone().into());
                }
                if let Some(desc) = &acp_link.description {
                    ann.insert("acp.description".into(), desc.clone().into());
                }
                let annotations = if ann.is_empty() {
                    None
                } else {
                    Some(serde_json::Value::Object(ann))
                };

                vec![ContentBlock::ResourceLink(ResourceLink {
                    uri: acp_link.uri.clone(),
                    name: acp_link.name.clone(),
                    media_type: acp_link.mime_type.clone(),
                    size: acp_link.size.map(|s| s as u64),
                    annotations,
                })]
            }
            // 未来可能的变体——不做丢弃，保持透传给 LLM 层处理
            _ => vec![],
        })
        .collect()
}

pub fn map_turn_stop(
    stop_reason: &StopReason,
    token_usage: Option<&TokenUsage>,
    model: Option<&Model>,
) -> Vec<SessionUpdate> {
    let reason_str = match stop_reason {
        StopReason::EndTurn => "end_turn",
        StopReason::MaxTokens => "max_tokens",
        StopReason::MaxTurnRequests => "max_turn_requests",
        StopReason::Refusal => "refusal",
        StopReason::Cancelled => "cancelled",
        StopReason::Error(msg) => {
            return vec![SessionUpdate::ToolCall(
                ToolCall::new("error".to_string(), msg.clone())
                    .kind(agent_client_protocol::schema::v1::ToolKind::Other)
                    .status(ToolCallStatus::Failed),
            )];
        }
    };

    let meta = Meta::from_iter([("stop_reason".into(), reason_str.into())]);

    let mut updates = vec![SessionUpdate::SessionInfoUpdate(
        SessionInfoUpdate::new().meta(Some(meta)),
    )];

    if let Some(usage) = token_usage {
        let used = usage.total_token_count as u64;
        let context_window = model.map(|m| m.context_window as u64).unwrap_or(used);
        let mut uu = UsageUpdate::new(used, context_window);

        if let Some(m) = model {
            let cost_amount = m.cost.compute(&playpen_config::model::Usage {
                input: usage.prompt_token_count.max(0) as usize,
                output: usage.candidates_token_count.max(0) as usize,
                cache_read: usage.cache_read_input_token_count.unwrap_or(0).max(0) as usize,
                cache_write: usage.cache_creation_input_token_count.unwrap_or(0).max(0) as usize,
                total: usage.total_token_count.max(0) as usize,
            });
            if cost_amount > 0.0 {
                let currency = match m.cost.currency {
                    playpen_config::model::Currency::CNY => "CNY",
                    playpen_config::model::Currency::USD => "USD",
                };
                uu = uu.cost(AcpCost::new(cost_amount, currency));
            }
        }

        updates.push(SessionUpdate::UsageUpdate(uu));
    }

    updates
}

#[cfg(test)]
#[path = "acp_content_test.rs"]
mod tests;
