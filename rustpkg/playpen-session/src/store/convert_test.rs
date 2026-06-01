use super::*;
use playpen_content::{ContentBlock, StopReason};

fn compress_json(value: &serde_json::Value) -> Vec<u8> {
    let json = serde_json::to_vec(value).unwrap();
    zstd::encode_all(std::io::Cursor::new(json), 3).unwrap()
}

fn compress_str(s: &str) -> Vec<u8> {
    compress_json(&serde_json::json!(s))
}

// ── event_role ──

#[test]
fn test_event_role_user() {
    let e = Event::UserMessage {
        id: String::new(),
        content: vec![],
    };
    assert_eq!(event_role(&e), "user");
}

#[test]
fn test_event_role_model() {
    let e = Event::ModelMessage {
        id: String::new(),
        content: vec![],
    };
    assert_eq!(event_role(&e), "model");
    assert_eq!(
        event_role(&Event::ModelMessageDelta {
            id: String::new(),
            text: "".into()
        }),
        "model"
    );
    assert_eq!(
        event_role(&Event::ModelThought {
            id: String::new(),
            text: "".into()
        }),
        "model"
    );
    assert_eq!(
        event_role(&Event::ModelThoughtDelta {
            id: String::new(),
            text: "".into()
        }),
        "model"
    );
}

#[test]
fn test_event_role_function() {
    assert_eq!(
        event_role(&Event::FunctionCall {
            id: String::new(),
            call_id: "".into(),
            name: "".into(),
            args: serde_json::json!({})
        }),
        "function"
    );
    assert_eq!(
        event_role(&Event::FunctionResult {
            id: String::new(),
            call_id: "".into(),
            name: "".into(),
            content: None,
            code: None
        }),
        "function"
    );
    assert_eq!(
        event_role(&Event::FunctionOutputDelta {
            id: String::new(),
            call_id: "".into(),
            name: "".into(),
            text: "".into()
        }),
        "function"
    );
}

#[test]
fn test_event_role_turn() {
    assert_eq!(
        event_role(&Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None
        }),
        "turn"
    );
}

#[test]
fn test_event_role_state() {
    assert_eq!(
        event_role(&Event::StateUpdate {
            id: "".into(),
            name: "".into(),
            data: serde_json::json!(null)
        }),
        "state"
    );
}

// ── normalize_event ──

#[test]
fn test_normalize_delta_returns_none() {
    assert!(
        normalize_event(&Event::ModelMessageDelta {
            id: String::new(),
            text: "x".into()
        })
        .is_none()
    );
    assert!(
        normalize_event(&Event::ModelThoughtDelta {
            id: String::new(),
            text: "x".into()
        })
        .is_none()
    );
    assert!(
        normalize_event(&Event::FunctionOutputDelta {
            id: String::new(),
            call_id: "".into(),
            name: "".into(),
            text: "".into()
        })
        .is_none()
    );
}

#[test]
fn test_normalize_state_update() {
    let n = normalize_event(&Event::StateUpdate {
        id: String::new(),
        name: "my_key".into(),
        data: serde_json::json!("my_val"),
    })
    .unwrap();
    assert_eq!(n.role, "state");
    assert_eq!(n.kind, "state_update");
    assert_eq!(n.name.as_deref(), Some("my_key"));
    assert_eq!(n.payload, serde_json::json!("my_val"));
}

#[test]
fn test_normalize_turn_stop() {
    let n = normalize_event(&Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    })
    .unwrap();
    assert_eq!(n.role, "turn");
    assert_eq!(n.kind, "stop_reason");
    assert!(n.name.is_none());
    assert_eq!(n.payload["stop_reason"], serde_json::json!("EndTurn"));
}

#[test]
fn test_normalize_turn_stop_with_token_usage() {
    let usage = playpen_content::TokenUsage {
        prompt_token_count: 1,
        candidates_token_count: 2,
        total_token_count: 3,
        cache_read_input_token_count: None,
        cache_creation_input_token_count: None,
        thinking_token_count: None,
    };
    let n = normalize_event(&Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::MaxTokens,
        token_usage: Some(usage),
    })
    .unwrap();
    assert_eq!(n.payload["stop_reason"], serde_json::json!("MaxTokens"));
    assert!(n.payload.get("token_usage").is_some());
}

#[test]
fn test_normalize_user_message() {
    let n = normalize_event(&Event::UserMessage {
        id: "msg1".into(),
        content: vec![ContentBlock::text("hi")],
    })
    .unwrap();
    assert_eq!(n.role, "user");
    assert_eq!(n.kind, "message");
    assert_eq!(n.payload["role"], "user");
}

#[test]
fn test_normalize_model_message() {
    let n = normalize_event(&Event::ModelMessage {
        id: String::new(),
        content: vec![ContentBlock::text("hello")],
    })
    .unwrap();
    assert_eq!(n.role, "model");
    assert_eq!(n.kind, "message");
    assert_eq!(n.payload["role"], "model");
}

#[test]
fn test_normalize_model_thought() {
    let n = normalize_event(&Event::ModelThought {
        id: String::new(),
        text: "reasoning...".into(),
    })
    .unwrap();
    assert_eq!(n.role, "model");
    assert_eq!(n.kind, "thinking");
    assert_eq!(n.payload["text"], "reasoning...");
}

#[test]
fn test_normalize_function_call() {
    let n = normalize_event(&Event::FunctionCall {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        args: serde_json::json!({"cmd": "ls"}),
    })
    .unwrap();
    assert_eq!(n.role, "function");
    assert_eq!(n.kind, "function_call");
    assert_eq!(n.name.as_deref(), Some("bash"));
    assert_eq!(n.payload["args"]["cmd"], "ls");
}

#[test]
fn test_normalize_function_result() {
    let n = normalize_event(&Event::FunctionResult {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        content: Some(vec![ContentBlock::text("ok")]),
        code: Some(0),
    })
    .unwrap();
    assert_eq!(n.role, "function");
    assert_eq!(n.kind, "function_result");
    assert_eq!(n.name.as_deref(), Some("bash"));
    assert_eq!(n.payload["code"], 0);
}

// ── denormalize_events ──

#[test]
fn test_denormalize_user_message() {
    let data = compress_json(&serde_json::json!({
        "role": "user", "content": [{"type": "text", "text": "hi"}], "id": "msg1"
    }));
    let events = denormalize_events("message", "user", None, &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::UserMessage { id, content } => {
            assert_eq!(id.as_str(), "msg1");
            assert!(!content.is_empty());
        }
        _ => panic!("期望 UserMessage"),
    }
}

#[test]
fn test_denormalize_model_message() {
    let data = compress_json(&serde_json::json!({
        "role": "model", "content": [{"type": "text", "text": "hello"}], "id": "m1"
    }));
    let events = denormalize_events("message", "model", None, &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ModelMessage { content, .. } => assert!(!content.is_empty()),
        _ => panic!("期望 ModelMessage"),
    }
}

#[test]
fn test_denormalize_model_thought() {
    let data = compress_json(&serde_json::json!({"text": "think...", "id": null}));
    let events = denormalize_events("thinking", "model", None, &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ModelThought { text, .. } => assert_eq!(text, "think..."),
        _ => panic!("期望 ModelThought"),
    }
}

#[test]
fn test_denormalize_function_call_with_name() {
    let data = compress_json(&serde_json::json!({"id": "c1", "args": {"cmd": "ls"}}));
    let events = denormalize_events("function_call", "function", Some("bash"), &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::FunctionCall {
            call_id,
            name,
            args,
            ..
        } => {
            assert_eq!(call_id, "c1");
            assert_eq!(name, "bash");
            assert_eq!(args["cmd"], "ls");
        }
        _ => panic!("期望 FunctionCall"),
    }
}

#[test]
fn test_denormalize_function_result() {
    let data = compress_json(&serde_json::json!({
        "call_id": "c1", "content": [{"type": "text", "text": "ok"}], "code": 0
    }));
    let events = denormalize_events("function_result", "function", Some("bash"), &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::FunctionResult {
            call_id,
            name,
            content,
            code,
            ..
        } => {
            assert_eq!(call_id, "c1");
            assert_eq!(name, "bash");
            assert!(content.is_some());
            assert_eq!(*code, Some(0));
        }
        _ => panic!("期望 FunctionResult"),
    }
}

#[test]
fn test_denormalize_turn_stop() {
    let data = compress_json(&serde_json::json!({"stop_reason": "EndTurn"}));
    let events = denormalize_events("stop_reason", "turn", None, &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::TurnStop {
            stop_reason,
            token_usage,
            ..
        } => {
            assert_eq!(*stop_reason, StopReason::EndTurn);
            assert!(token_usage.is_none());
        }
        _ => panic!("期望 TurnStop"),
    }
}

#[test]
fn test_denormalize_turn_stop_with_token_usage() {
    let data = compress_json(&serde_json::json!({
        "stop_reason": "MaxTokens",
        "token_usage": {"prompt_token_count": 1, "candidates_token_count": 2, "total_token_count": 3}
    }));
    let events = denormalize_events("stop_reason", "turn", None, &data, "e1");
    match &events[0] {
        Event::TurnStop {
            stop_reason,
            token_usage,
            ..
        } => {
            assert_eq!(*stop_reason, StopReason::MaxTokens);
            assert!(token_usage.is_some());
        }
        _ => panic!("期望 TurnStop"),
    }
}

#[test]
fn test_denormalize_state_update() {
    let data = compress_json(&serde_json::json!("my_value"));
    let events = denormalize_events("state_update", "state", Some("my_key"), &data, "e1");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::StateUpdate { name, data, .. } => {
            assert_eq!(name, "my_key");
            assert_eq!(data.as_str(), Some("my_value"));
        }
        _ => panic!("期望 StateUpdate"),
    }
}

#[test]
fn test_denormalize_wrong_role_kind_returns_empty() {
    let data = compress_json(&serde_json::json!("x"));
    assert!(denormalize_events("message", "turn", None, &data, "e1").is_empty());
    assert!(denormalize_events("thinking", "user", None, &data, "e1").is_empty());
    assert!(denormalize_events("function_call", "model", None, &data, "e1").is_empty());
    assert!(denormalize_events("stop_reason", "function", None, &data, "e1").is_empty());
    assert!(denormalize_events("state_update", "turn", None, &data, "e1").is_empty());
}

// ── decode_state_data ──

#[test]
fn test_decode_state_data_valid() {
    let data = compress_str("hello");
    let result: String = decode_state_data(&data).unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn test_decode_state_data_invalid() {
    let corrupted = vec![0, 1, 2];
    assert!(decode_state_data::<String>(&corrupted).is_err());
}

#[test]
fn test_decode_state_data_json_object() {
    let data = compress_json(&serde_json::json!({"a": 1, "b": 2}));
    let result: serde_json::Value = decode_state_data(&data).unwrap();
    assert_eq!(result["a"], 1);
}
