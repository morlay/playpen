use super::*;
use futures::StreamExt;
use playpen_content::{ContentBlock, Resource, StopReason};
use rig_core::completion::FinishReason;
use rig_core::completion::message::{ToolCall, ToolCallId};
use rig_core::streaming::StreamedAssistantContent;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 记录上传内容的 mock uploader。
struct MockImageUploader {
    uploaded: Arc<Mutex<Vec<UploadRecord>>>,
}

/// 单次上传记录：(name, media_type, data)。
type UploadRecord = (String, String, Vec<u8>);

impl MockImageUploader {
    fn new() -> Self {
        Self {
            uploaded: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl ImageUploader for MockImageUploader {
    async fn upload_image(
        &self,
        name: &str,
        media_type: &str,
        data: Vec<u8>,
    ) -> anyhow::Result<String> {
        self.uploaded
            .lock()
            .await
            .push((name.to_string(), media_type.to_string(), data.clone()));
        Ok(format!("file-api-{}", data.len()))
    }

    async fn read_image_file(&self, uri: &str) -> anyhow::Result<Vec<u8>> {
        Ok(format!("bytes-of-{uri}").into_bytes())
    }
}

#[tokio::test]
async fn test_user_message_to_rig() {
    let event = Event::UserMessage {
        id: "u1".to_string(),
        content: vec![ContentBlock::text("hello")],
    };
    let msg = event_to_user_message(&event, None).await.unwrap();
    match msg {
        Message::User { content } => {
            let first = content.first().unwrap();
            match first {
                UserContent::Text(t) => assert_eq!(t.text, "hello"),
                _ => panic!("期望 Text"),
            }
        }
        _ => panic!("期望 User"),
    }
}

#[test]
fn test_model_message_to_assistant_content() {
    let event = Event::ModelMessage {
        id: "m1".to_string(),
        content: vec![ContentBlock::text("hi there")],
    };
    let content = event_to_assistant_content(&event).unwrap();
    match content {
        AssistantContent::Text(t) => assert_eq!(t.text, "hi there"),
        _ => panic!("期望 Text"),
    }
}

#[test]
fn test_function_call_to_assistant_content() {
    let event = Event::FunctionCall {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        args: serde_json::json!({"command": "echo hi"}),
    };
    let content = event_to_assistant_content(&event).unwrap();
    match content {
        AssistantContent::ToolCall(tc) => {
            assert_eq!(tc.function.name, "bash");
            assert_eq!(tc.function.arguments["command"], "echo hi");
        }
        _ => panic!("期望 ToolCall"),
    }
}

#[test]
fn test_function_result_to_tool_result() {
    let event = Event::FunctionResult {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        content: Some(vec![ContentBlock::text("hi\n")]),
        code: Some(0),
    };
    let msg = event_to_tool_result(&event).unwrap();
    match msg {
        Message::User { content } => {
            let first = content.first().unwrap();
            match first {
                UserContent::ToolResult(_tr) => {
                    assert!(!_tr.content.is_empty());
                    // Text 块应原样保留
                    let s = serde_json::to_string(&_tr.content).unwrap();
                    assert!(s.contains("hi"), "结果应包含 hi，实际: {s}");
                }
                _ => panic!("期望 ToolResult"),
            }
        }
        _ => panic!("期望 User"),
    }
}

/// bash/read/grep 工具返回 Resource::Text 块（/dev/stdout 等），
/// 结果必须传给 LLM，不能因类型过滤而变成空串。
#[test]
fn test_function_result_resource_text_to_tool_result() {
    let event = Event::FunctionResult {
        id: String::new(),
        call_id: "c2".into(),
        name: "bash".into(),
        content: Some(vec![ContentBlock::resource(Resource::text(
            "/dev/stdout",
            "text/plain",
            "hello from bash\n",
        ))]),
        code: Some(0),
    };
    let msg = event_to_tool_result(&event).unwrap();
    match msg {
        Message::User { content } => {
            let UserContent::ToolResult(tr) = content.first().unwrap() else {
                panic!("期望 ToolResult");
            };
            let s = serde_json::to_string(&tr.content).unwrap();
            assert!(
                s.contains("hello from bash"),
                "Resource 块结果应包含输出文本，实际: {s}"
            );
        }
        _ => panic!("期望 User"),
    }
}

#[test]
fn test_delta_skipped() {
    let event = Event::ModelMessageDelta {
        id: String::new(),
        text: "partial".into(),
    };
    assert!(event_to_assistant_content(&event).is_none());

    let event = Event::ModelThoughtDelta {
        id: String::new(),
        text: "thinking".into(),
    };
    assert!(event_to_assistant_content(&event).is_none());
}

#[tokio::test]
async fn test_turn_stop_skipped() {
    let event = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    assert!(event_to_assistant_content(&event).is_none());
    assert!(event_to_user_message(&event, None).await.is_none());
    assert!(event_to_tool_result(&event).is_none());
}

#[tokio::test]
async fn test_events_to_chat_history_merges_assistant() {
    let events = vec![
        Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("hello")],
        },
        Event::ModelThought {
            id: String::new(),
            text: "thinking...".into(),
        },
        Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("world")],
        },
        Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "read".into(),
            args: serde_json::json!({"path": "main.rs"}),
        },
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
    ];

    let msgs: Vec<Message> = futures::stream::iter(events)
        .pipe(|stream| events_to_chat_history(stream, None))
        .collect()
        .await;

    assert_eq!(msgs.len(), 2, "User + 合并的 Assistant");
    assert!(matches!(&msgs[0], Message::User { .. }));
    match &msgs[1] {
        Message::Assistant { content, .. } => {
            let items: Vec<_> = content.iter().collect();
            assert_eq!(items.len(), 3, "应包含 3 个 AssistantContent");
            assert!(matches!(items[0], AssistantContent::Reasoning(_)));
            assert!(matches!(items[1], AssistantContent::Text(_)));
            assert!(matches!(items[2], AssistantContent::ToolCall(_)));
        }
        _ => panic!("期望 Assistant"),
    }
}

#[tokio::test]
async fn test_orphan_function_result_dropped() {
    let events = vec![
        Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "read".into(),
            args: serde_json::json!({"path": "main.rs"}),
        },
        Event::FunctionResult {
            id: String::new(),
            call_id: "c1".into(),
            name: "read".into(),
            content: Some(vec![ContentBlock::text("ok")]),
            code: Some(0),
        },
        // 孤儿：有 call_id 但无对应的 FunctionCall，应被丢弃
        Event::FunctionResult {
            id: String::new(),
            call_id: "orphan".into(),
            name: "read".into(),
            content: Some(vec![ContentBlock::text("stray")]),
            code: Some(0),
        },
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
    ];

    let msgs: Vec<Message> = futures::stream::iter(events)
        .pipe(|stream| events_to_chat_history(stream, None))
        .collect()
        .await;

    // Assistant(ToolCall c1) + User(ToolResult c1)，孤儿 result 不产生消息
    assert_eq!(msgs.len(), 2, "孤儿 FunctionResult 不应产生消息");

    let tool_result_ids: Vec<String> = msgs
        .iter()
        .filter_map(|m| match m {
            Message::User { content } => content.iter().find_map(|c| match c {
                UserContent::ToolResult(tr) => Some(tr.call.as_str().to_string()),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_result_ids,
        vec!["c1".to_string()],
        "仅配对的 ToolResult 应传给 LLM"
    );
}

#[tokio::test]
async fn test_empty_content_skipped() {
    let event = Event::UserMessage {
        id: String::new(),
        content: vec![],
    };
    assert!(event_to_user_message(&event, None).await.is_none());

    let event = Event::ModelMessage {
        id: String::new(),
        content: vec![],
    };
    assert!(event_to_assistant_content(&event).is_none());
}

// ── 图片上传转换 ──

#[test]
fn test_is_image_block() {
    let blob = ContentBlock::Resource(Resource::Blob {
        uri: "file:///tmp/a.png".into(),
        media_type: "image/png".into(),
        blob: vec![1],
        annotations: None,
    });
    assert!(is_image_block(&blob));

    let blob_txt = ContentBlock::Resource(Resource::Blob {
        uri: "file:///tmp/a.txt".into(),
        media_type: "text/plain".into(),
        blob: vec![1],
        annotations: None,
    });
    assert!(!is_image_block(&blob_txt));

    let link = ContentBlock::ResourceLink(playpen_content::ResourceLink {
        uri: "file:///tmp/a.jpg".into(),
        name: "a.jpg".into(),
        media_type: None,
        size: None,
        annotations: None,
    });
    assert!(is_image_block(&link));

    let link_pdf = ContentBlock::ResourceLink(playpen_content::ResourceLink {
        uri: "file:///tmp/a.pdf".into(),
        name: "a.pdf".into(),
        media_type: None,
        size: None,
        annotations: None,
    });
    assert!(!is_image_block(&link_pdf));
}

#[tokio::test]
async fn test_user_message_with_image_uploads_file() {
    let mock_uploader = Arc::new(MockImageUploader::new());
    let uploader: Arc<dyn ImageUploader> = mock_uploader.clone();
    let event = Event::UserMessage {
        id: String::new(),
        content: vec![
            ContentBlock::text("what is this?"),
            ContentBlock::Resource(Resource::Blob {
                uri: "file:///tmp/cat.png".into(),
                media_type: "image/png".into(),
                blob: vec![1, 2, 3],
                annotations: None,
            }),
        ],
    };
    let msg = event_to_user_message(&event, Some(&uploader))
        .await
        .unwrap();
    match msg {
        Message::User { content } => {
            assert_eq!(content.len(), 2);
            assert!(matches!(content[0], UserContent::Text(_)));
            match &content[1] {
                UserContent::Document(doc) => {
                    assert_eq!(
                        doc.data,
                        DocumentSourceKind::FileId("file-api-3".to_string())
                    );
                }
                _ => panic!("期望 Document(FileId)"),
            }
        }
        _ => panic!("期望 User"),
    }
    let uploaded = mock_uploader.uploaded.lock().await;
    assert_eq!(uploaded.len(), 1);
    assert_eq!(uploaded[0].0, "cat.png");
    assert_eq!(uploaded[0].1, "image/png");
}

#[tokio::test]
async fn test_user_message_without_uploader_keeps_text() {
    let event = Event::UserMessage {
        id: String::new(),
        content: vec![ContentBlock::Resource(Resource::Blob {
            uri: "file:///tmp/cat.png".into(),
            media_type: "image/png".into(),
            blob: vec![1, 2, 3],
            annotations: None,
        })],
    };
    let msg = event_to_user_message(&event, None).await.unwrap();
    match msg {
        Message::User { content } => {
            assert!(
                matches!(content[0], UserContent::Text(_)),
                "无 uploader 时应文本化"
            );
        }
        _ => panic!("期望 User"),
    }
}

#[tokio::test]
async fn test_events_to_chat_history_uploads_image() {
    let uploader = Arc::new(MockImageUploader::new());
    let events = vec![
        Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::Resource(Resource::Blob {
                uri: "file:///tmp/cat.png".into(),
                media_type: "image/png".into(),
                blob: vec![9, 9],
                annotations: None,
            })],
        },
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
    ];
    let msgs: Vec<Message> = futures::stream::iter(events)
        .pipe(|stream| events_to_chat_history(stream, Some(uploader)))
        .collect()
        .await;
    match &msgs[0] {
        Message::User { content } => {
            assert!(matches!(content[0], UserContent::Document(_)));
        }
        _ => panic!("期望 User"),
    }
}

// ── process_stream ──

#[tokio::test]
async fn test_process_stream_text_delta_shares_id() {
    let items: Vec<Result<StreamedAssistantContent, String>> = vec![
        Ok(StreamedAssistantContent::text("Hello")),
        Ok(StreamedAssistantContent::text(" World")),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream).collect().await;

    // 连续 text delta 应共享同一 id
    assert!(events.len() >= 3, "应有 delta + flush + turn_stop");

    let delta_id = match &events[0] {
        Event::ModelMessageDelta { id, text } => {
            assert_eq!(text, "Hello");
            assert!(!id.is_empty(), "delta id 不应为空");
            id.clone()
        }
        _ => panic!("第一个事件应为 ModelMessageDelta"),
    };

    match &events[1] {
        Event::ModelMessageDelta { id, text } => {
            assert_eq!(text, " World");
            assert_eq!(id, &delta_id, "连续 delta 应共享同一 id");
        }
        _ => panic!("第二个事件应为 ModelMessageDelta"),
    }

    // 流结束 flush 的 ModelMessage 应与 delta 同 id
    match &events[2] {
        Event::ModelMessage { id, .. } => {
            assert_eq!(id, &delta_id, "flush ModelMessage 应与 delta 共享同一 id");
        }
        _ => panic!("第三个事件应为 ModelMessage"),
    }

    // 最后应有 TurnStop
    assert!(
        events.iter().any(|e| matches!(e, Event::TurnStop { .. })),
        "应有 TurnStop"
    );
}

#[tokio::test]
async fn test_process_stream_text_and_thought_have_different_ids() {
    let items: Vec<Result<StreamedAssistantContent, String>> = vec![
        Ok(StreamedAssistantContent::text("Hello")),
        Ok(StreamedAssistantContent::ReasoningDelta {
            id: "reasoning-0".into(),
            provider_id: None,
            reasoning: "thinking...".into(),
        }),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream).collect().await;

    // 应有 text delta + thought delta + flush events + turn_stop
    assert!(events.len() >= 2);

    let text_id = match &events[0] {
        Event::ModelMessageDelta { id, text } => {
            assert_eq!(text, "Hello");
            id.clone()
        }
        _ => panic!("第一个事件应为 ModelMessageDelta"),
    };

    let thought_id = match &events[1] {
        Event::ModelThoughtDelta { id, .. } => {
            assert!(!id.is_empty(), "thought delta id 不应为空");
            id.clone()
        }
        _ => panic!("第二个事件应为 ModelThoughtDelta"),
    };

    assert_ne!(text_id, thought_id, "text 和 thought 的 id 应不同");
}

#[tokio::test]
async fn test_process_stream_tool_call_has_own_id() {
    let items: Vec<Result<StreamedAssistantContent, String>> = vec![
        Ok(StreamedAssistantContent::text("思考")),
        Ok(StreamedAssistantContent::ToolCall {
            tool_call: ToolCall::new(
                ToolCallId::new_or_mint("call_1"),
                rig_core::completion::message::ToolFunction::new(
                    "read".into(),
                    serde_json::json!({}),
                ),
            ),
            internal_call_id: "internal_1".into(),
        }),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream).collect().await;

    // 应有 text delta + flush ModelMessage + FunctionCall + TurnStop
    assert!(events.len() >= 3);

    let text_id = match &events[0] {
        Event::ModelMessageDelta { id, .. } => id.clone(),
        _ => panic!("第一个应为 ModelMessageDelta"),
    };

    // flush 的 ModelMessage 应与 delta 同 id
    match &events[1] {
        Event::ModelMessage { id, .. } => {
            assert_eq!(id, &text_id, "flush 应与 delta 同 id");
        }
        _ => panic!("第二个应为 ModelMessage"),
    }

    // FunctionCall 应有自己的 id
    match &events[2] {
        Event::FunctionCall { id, .. } => {
            assert!(!id.is_empty(), "FunctionCall id 不应为空");
            assert_ne!(id, &text_id, "FunctionCall id 应与 text id 不同");
        }
        _ => panic!("第三个应为 FunctionCall"),
    }
}

// ── finish_reason_to_stop_reason ──

#[test]
fn test_finish_reason_stop() {
    assert_eq!(
        finish_reason_to_stop_reason(Some(&FinishReason::Stop)),
        StopReason::EndTurn
    );
}

#[test]
fn test_finish_reason_length() {
    assert_eq!(
        finish_reason_to_stop_reason(Some(&FinishReason::Length)),
        StopReason::MaxTokens
    );
}

#[test]
fn test_finish_reason_refusal() {
    assert_eq!(
        finish_reason_to_stop_reason(Some(&FinishReason::ContentFilter)),
        StopReason::Refusal
    );
}

#[test]
fn test_finish_reason_tool_calls() {
    assert_eq!(
        finish_reason_to_stop_reason(Some(&FinishReason::ToolCalls)),
        StopReason::EndTurn
    );
}

#[test]
fn test_finish_reason_other() {
    assert_eq!(
        finish_reason_to_stop_reason(Some(&FinishReason::Other("x".into()))),
        StopReason::EndTurn
    );
}

#[test]
fn test_finish_reason_none() {
    assert_eq!(finish_reason_to_stop_reason(None), StopReason::EndTurn);
}

// ── usage_to_playpen ──

#[test]
fn test_usage_to_playpen() {
    let usage = rig_core::completion::Usage {
        input_tokens: 10,
        output_tokens: 5,
        total_tokens: 15,
        cached_input_tokens: 3,
        cache_creation_input_tokens: 2,
        tool_use_prompt_tokens: 0,
        reasoning_tokens: 1,
    };
    let tp = usage_to_playpen(&usage).unwrap();
    assert_eq!(tp.prompt_token_count, 10);
    assert_eq!(tp.candidates_token_count, 5);
    assert_eq!(tp.total_token_count, 15);
    assert_eq!(tp.cache_read_input_token_count, Some(3));
    assert_eq!(tp.cache_creation_input_token_count, Some(2));
    assert_eq!(tp.thinking_token_count, Some(1));
}

#[test]
fn test_usage_to_playpen_zero_returns_none() {
    let usage = rig_core::completion::Usage::new();
    assert!(usage_to_playpen(&usage).is_none());
}

// ── Final chunk ──

#[tokio::test]
async fn test_process_stream_final_sets_info() {
    use rig_core::streaming::{StreamFinal, StreamFinalKind};

    let items: Vec<Result<StreamedAssistantContent, String>> =
        vec![Ok(StreamedAssistantContent::Final(StreamFinal {
            kind: StreamFinalKind::Final,
            usage: rig_core::completion::Usage {
                input_tokens: 100,
                output_tokens: 20,
                total_tokens: 120,
                ..Default::default()
            },
            finish_reason: Some(FinishReason::Length),
            message_id: None,
            response_id: None,
            provider_request_id: None,
            provider: "deepseek".into(),
            model: None,
            raw: serde_json::Value::Null,
        }))];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream).collect().await;

    let stop = events
        .iter()
        .find_map(|e| match e {
            Event::TurnStop {
                stop_reason,
                token_usage,
                ..
            } => Some((stop_reason, token_usage)),
            _ => None,
        })
        .expect("应有 TurnStop");
    assert_eq!(*stop.0, StopReason::MaxTokens);
    let usage = stop.1.as_ref().unwrap();
    assert_eq!(usage.prompt_token_count, 100);
    assert_eq!(usage.candidates_token_count, 20);
}
