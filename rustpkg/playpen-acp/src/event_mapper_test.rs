use super::*;
use std::path::Path;

use playpen_content::{ContentBlock, Event, StopReason, TokenUsage};

// ── helpers ────────────────────────────────────────────────────────

fn mapper() -> EventMapper<'static> {
    EventMapper::new(Path::new("/workspace"))
}

fn mapper_term() -> EventMapper<'static> {
    EventMapper::new(Path::new("/workspace")).with_term_enabled(true)
}

fn mapper_replay() -> EventMapper<'static> {
    EventMapper::new(Path::new("/workspace")).with_replay(true)
}

fn first_text(updates: &[SessionUpdate]) -> Option<&str> {
    for u in updates {
        if let SessionUpdate::AgentMessageChunk(chunk) = u
            && let AcpContentBlock::Text(t) = &chunk.content
        {
            return Some(&t.text);
        }
        if let SessionUpdate::AgentThoughtChunk(chunk) = u
            && let AcpContentBlock::Text(t) = &chunk.content
        {
            return Some(&t.text);
        }
        if let SessionUpdate::UserMessageChunk(chunk) = u
            && let AcpContentBlock::Text(t) = &chunk.content
        {
            return Some(&t.text);
        }
    }
    None
}

fn has_tool_call(updates: &[SessionUpdate], status: ToolCallStatus) -> bool {
    updates.iter().any(|u| match u {
        SessionUpdate::ToolCall(tc) => tc.status == status,
        _ => false,
    })
}

fn has_tool_update(updates: &[SessionUpdate], status: ToolCallStatus) -> bool {
    updates.iter().any(|u| match u {
        SessionUpdate::ToolCallUpdate(tu) => tu.fields.status == Some(status),
        _ => false,
    })
}

// ── map_event: ModelMessageDelta (live) ────────────────────────────

#[test]
fn test_model_message_delta_in_live_mode_emits_chunk() {
    let event = Event::ModelMessageDelta {
        id: String::new(),
        text: "hello".into(),
    };
    let updates = mapper().map_event(&event);
    assert_eq!(updates.len(), 1);
    assert_eq!(first_text(&updates), Some("hello"));
}

#[test]
fn test_model_message_delta_in_replay_mode_skipped() {
    let event = Event::ModelMessageDelta {
        id: String::new(),
        text: "hello".into(),
    };
    let updates = mapper_replay().map_event(&event);
    assert!(updates.is_empty(), "replay 模式不应发射 delta");
}

// ── map_event: ModelMessage (replay) ───────────────────────────────

#[test]
fn test_model_message_in_replay_mode_emits_full_chunk() {
    let event = Event::ModelMessage {
        id: "m1".to_string(),
        content: vec![ContentBlock::text("full response")],
    };
    let updates = mapper_replay().map_event(&event);
    assert_eq!(updates.len(), 1);
    assert_eq!(first_text(&updates), Some("full response"));
}

#[test]
fn test_model_message_in_live_mode_skipped() {
    let event = Event::ModelMessage {
        id: String::new(),
        content: vec![],
    };
    let updates = mapper().map_event(&event);
    assert!(updates.is_empty(), "live 模式不应发射完整 message");
}

// ── map_event: ModelThoughtDelta ───────────────────────────────────

#[test]
fn test_thought_delta_in_live_mode_emits_chunk() {
    let event = Event::ModelThoughtDelta {
        id: String::new(),
        text: "thinking...".into(),
    };
    let updates = mapper().map_event(&event);
    assert_eq!(updates.len(), 1);
    assert_eq!(first_text(&updates), Some("thinking..."));
}

#[test]
fn test_thought_delta_in_replay_mode_skipped() {
    let event = Event::ModelThoughtDelta {
        id: String::new(),
        text: "thinking...".into(),
    };
    let updates = mapper_replay().map_event(&event);
    assert!(updates.is_empty(), "replay 模式不应发射 thought delta");
}

// ── map_event: FunctionCall (bash + term_enabled) ──────────────────

#[test]
fn test_function_call_bash_with_term_emits_tool_call_with_terminal() {
    let event = Event::FunctionCall {
        id: "fc1".to_string(),
        call_id: "call_1".into(),
        name: "bash".into(),
        args: serde_json::json!({"command": "echo hi"}),
    };
    let updates = mapper_term().map_event(&event);
    assert_eq!(updates.len(), 1);
    assert!(has_tool_call(&updates, ToolCallStatus::InProgress));
    if let Some(SessionUpdate::ToolCall(tc)) = updates.first() {
        assert!(
            tc.content
                .iter()
                .any(|c| matches!(c, ToolCallContent::Terminal(_)))
        );
    }
}

#[test]
fn test_function_call_non_bash_emits_tool_call_pending() {
    let event = Event::FunctionCall {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        args: serde_json::json!({"path": "main.rs"}),
    };
    let updates = mapper().map_event(&event);
    assert_eq!(updates.len(), 2);
    assert!(has_tool_call(&updates, ToolCallStatus::Pending));
    assert!(has_tool_update(&updates, ToolCallStatus::InProgress));
}

// ── map_event: FunctionResult (success / error) ────────────────────

#[test]
fn test_function_result_success_emits_completed() {
    let event = Event::FunctionResult {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        content: Some(vec![ContentBlock::text("file content")]),
        code: Some(0),
    };
    let updates = mapper().map_event(&event);
    assert!(
        has_tool_update(&updates, ToolCallStatus::Completed),
        "exit_code=0 应为 Completed"
    );
}

#[test]
fn test_function_result_error_emits_failed() {
    let event = Event::FunctionResult {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        content: Some(vec![ContentBlock::text("Error: not found")]),
        code: Some(1),
    };
    let updates = mapper().map_event(&event);
    assert!(
        has_tool_update(&updates, ToolCallStatus::Failed),
        "exit_code=1 应为 Failed"
    );
}

#[test]
fn test_function_result_error_text_detection() {
    let event = Event::FunctionResult {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        content: Some(vec![ContentBlock::text("Error: something went wrong")]),
        code: None,
    };
    let updates = mapper().map_event(&event);
    assert!(
        has_tool_update(&updates, ToolCallStatus::Failed),
        "Error 前缀文本应判定为 Failed"
    );
}

// ── map_event: TurnStop ───────────────────────────────────────────

#[test]
fn test_turn_stop_end_turn_emits_info_update() {
    let event = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    let updates = mapper().map_event(&event);
    assert!(!updates.is_empty());
    assert!(matches!(&updates[0], SessionUpdate::SessionInfoUpdate(_)));
}

#[test]
fn test_turn_stop_with_token_usage_emits_usage_update() {
    let event = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: Some(TokenUsage {
            total_token_count: 100,
            prompt_token_count: 50,
            candidates_token_count: 50,
            cache_read_input_token_count: None,
            cache_creation_input_token_count: None,
            thinking_token_count: None,
        }),
    };
    let updates = mapper().map_event(&event);
    assert_eq!(updates.len(), 2);
    assert!(matches!(&updates[1], SessionUpdate::UsageUpdate(_)));
}

#[test]
fn test_turn_stop_error_emits_fake_tool_call() {
    let event = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::Error("something failed".into()),
        token_usage: None,
    };
    let updates = mapper().map_event(&event);
    assert_eq!(
        updates.len(),
        1,
        "Error stop_reason 应产生 1 个 fake tool call"
    );
    if let Some(agent_client_protocol::schema::v1::SessionUpdate::ToolCall(tc)) = updates.first() {
        assert_eq!(
            tc.status,
            agent_client_protocol::schema::v1::ToolCallStatus::Failed
        );
    } else {
        panic!("期望 SessionUpdate::ToolCall");
    }
}

// ── map_event: StateUpdate ─────────────────────────────────────────

#[test]
fn test_state_update_always_empty() {
    let event = Event::StateUpdate {
        id: "s1".into(),
        name: "key".into(),
        data: serde_json::json!("value"),
    };
    let updates = mapper().map_event(&event);
    assert!(updates.is_empty(), "StateUpdate 不应产生 SessionUpdate");
}

// ── map_event: UserMessage (replay only) ──────────────────────────

#[test]
fn test_user_message_in_replay_mode_emits_chunk() {
    let event = Event::UserMessage {
        id: "u1".to_string(),
        content: vec![ContentBlock::text("user input")],
    };
    let updates = mapper_replay().map_event(&event);
    assert_eq!(updates.len(), 1);
    assert_eq!(first_text(&updates), Some("user input"));
}

#[test]
fn test_user_message_in_live_mode_skipped() {
    let event = Event::UserMessage {
        id: String::new(),
        content: vec![ContentBlock::text("user input")],
    };
    let updates = mapper().map_event(&event);
    assert!(updates.is_empty(), "live 模式不应发射 UserMessage");
}

// ── map_event: FunctionOutputDelta ─────────────────────────────────

#[test]
fn test_function_output_delta_emits_tool_call_update() {
    let event = Event::FunctionOutputDelta {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        text: "output text".into(),
    };
    let updates = mapper().map_event(&event);
    assert!(!updates.is_empty());
    assert!(matches!(&updates[0], SessionUpdate::ToolCallUpdate(_)));
}

#[test]
fn test_function_output_delta_replay_skipped() {
    let event = Event::FunctionOutputDelta {
        id: String::new(),
        call_id: "call_1".into(),
        name: "read".into(),
        text: "output text".into(),
    };
    let updates = mapper_replay().map_event(&event);
    assert!(updates.is_empty(), "replay 模式不应发射 delta");
}

// ── with_default_message_id ────────────────────────────────────────

#[test]
fn test_default_message_id_not_needed_when_event_has_id() {
    let event = Event::ModelMessageDelta {
        id: "delta-123".into(),
        text: "chunk".into(),
    };
    // 现在所有事件都有 id，应使用事件自身的 id 而非 fallback
    let mapper = EventMapper::new(Path::new("/workspace")).with_default_message_id("fallback");
    let updates = mapper.map_event(&event);
    assert!(!updates.is_empty());
    if let Some(SessionUpdate::AgentMessageChunk(chunk)) = updates.first() {
        assert_eq!(
            chunk.message_id.as_ref().map(|id| id.to_string()),
            Some("delta-123".into())
        );
    } else {
        panic!("期望 AgentMessageChunk");
    }
}
