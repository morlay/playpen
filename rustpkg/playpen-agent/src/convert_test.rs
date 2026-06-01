use super::*;
use futures::StreamExt;
use playpen_content::{ContentBlock, StopReason};
use rig_core::streaming::StreamedAssistantContent;
use rig_core::test_utils::MockResponse;

#[test]
fn test_user_message_to_rig() {
    let event = Event::UserMessage {
        id: "u1".to_string(),
        content: vec![ContentBlock::text("hello")],
    };
    let msg = event_to_user_message(&event).unwrap();
    match msg {
        Message::User { content } => {
            let first = content.first_ref();
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
            let first = content.first_ref();
            match first {
                UserContent::ToolResult(_tr) => {
                    assert!(!_tr.content.is_empty());
                }
                _ => panic!("期望 ToolResult"),
            }
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

#[test]
fn test_turn_stop_skipped() {
    let event = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    assert!(event_to_assistant_content(&event).is_none());
    assert!(event_to_user_message(&event).is_none());
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
        .pipe(events_to_chat_history)
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

#[test]
fn test_empty_content_skipped() {
    let event = Event::UserMessage {
        id: String::new(),
        content: vec![],
    };
    assert!(event_to_user_message(&event).is_none());

    let event = Event::ModelMessage {
        id: String::new(),
        content: vec![],
    };
    assert!(event_to_assistant_content(&event).is_none());
}

// ── process_stream ──

#[tokio::test]
async fn test_process_stream_text_delta_shares_id() {
    let items: Vec<Result<StreamedAssistantContent<MockResponse>, String>> = vec![
        Ok(StreamedAssistantContent::text("Hello")),
        Ok(StreamedAssistantContent::text(" World")),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream, |_| None).collect().await;

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
    let items: Vec<Result<StreamedAssistantContent<MockResponse>, String>> = vec![
        Ok(StreamedAssistantContent::text("Hello")),
        Ok(StreamedAssistantContent::ReasoningDelta {
            id: None,
            reasoning: "thinking...".into(),
        }),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream, |_| None).collect().await;

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
    let items: Vec<Result<StreamedAssistantContent<MockResponse>, String>> = vec![
        Ok(StreamedAssistantContent::text("思考")),
        Ok(StreamedAssistantContent::ToolCall {
            tool_call: rig_core::completion::message::ToolCall::new(
                "call_1".into(),
                rig_core::completion::message::ToolFunction::new(
                    "read".into(),
                    serde_json::json!({}),
                ),
            ),
            internal_call_id: "internal_1".into(),
        }),
    ];
    let stream = futures::stream::iter(items);
    let events: Vec<Event> = process_stream(stream, |_| None).collect().await;

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
        crate::convert::finish_reason_to_stop_reason(Some("stop")),
        StopReason::EndTurn
    );
}

#[test]
fn test_finish_reason_length() {
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("length")),
        StopReason::MaxTokens
    );
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("max_tokens")),
        StopReason::MaxTokens
    );
}

#[test]
fn test_finish_reason_refusal() {
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("refusal")),
        StopReason::Refusal
    );
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("content_filter")),
        StopReason::Refusal
    );
}

#[test]
fn test_finish_reason_unknown() {
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("tool_calls")),
        StopReason::EndTurn
    );
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(Some("other")),
        StopReason::EndTurn
    );
}

#[test]
fn test_finish_reason_none() {
    assert_eq!(
        crate::convert::finish_reason_to_stop_reason(None),
        StopReason::EndTurn
    );
}
