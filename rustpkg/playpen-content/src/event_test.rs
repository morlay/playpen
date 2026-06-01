use super::*;
use crate::content::ContentBlock;

#[test]
fn test_user_message_event_id() {
    let e = Event::UserMessage {
        id: "e1".to_string(),
        content: vec![ContentBlock::text("hi")],
    };
    assert_eq!(e.event_id(), Some("e1"));

    let e = Event::UserMessage {
        id: String::new(),
        content: vec![],
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_model_message_event_id() {
    let e = Event::ModelMessage {
        id: "e1".to_string(),
        content: vec![ContentBlock::text("hi")],
    };
    assert_eq!(e.event_id(), Some("e1"));

    let e = Event::ModelMessage {
        id: String::new(),
        content: vec![],
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_model_thought_event_id() {
    let e = Event::ModelThought {
        id: "t1".to_string(),
        text: "think".into(),
    };
    assert_eq!(e.event_id(), Some("t1"));

    let e = Event::ModelThought {
        id: String::new(),
        text: String::new(),
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_function_call_event_id() {
    let e = Event::FunctionCall {
        id: "fc1".to_string(),
        call_id: "call_1".into(),
        name: "bash".into(),
        args: serde_json::json!({"cmd": "ls"}),
    };
    assert_eq!(e.event_id(), Some("fc1"));

    let e = Event::FunctionCall {
        id: String::new(),
        call_id: "call_1".into(),
        name: "bash".into(),
        args: serde_json::json!({}),
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_function_output_delta_event_id() {
    let e = Event::FunctionOutputDelta {
        id: "d1".to_string(),
        call_id: "call_1".into(),
        name: "bash".into(),
        text: "out".into(),
    };
    assert_eq!(e.event_id(), Some("d1"));

    let e = Event::FunctionOutputDelta {
        id: String::new(),
        call_id: "call_1".into(),
        name: "bash".into(),
        text: String::new(),
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_function_result_event_id() {
    let e = Event::FunctionResult {
        id: "fr1".to_string(),
        call_id: "call_1".into(),
        name: "bash".into(),
        content: Some(vec![ContentBlock::text("ok")]),
        code: Some(0),
    };
    assert_eq!(e.event_id(), Some("fr1"));

    let e = Event::FunctionResult {
        id: String::new(),
        call_id: "call_1".into(),
        name: "bash".into(),
        content: None,
        code: None,
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_turn_stop_event_id() {
    let e = Event::TurnStop {
        id: "ts1".to_string(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    assert_eq!(e.event_id(), Some("ts1"));

    let e = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    assert_eq!(e.event_id(), Some(""));
}

#[test]
fn test_state_update_event_id() {
    let e = Event::StateUpdate {
        id: "st1".into(),
        name: "key".into(),
        data: serde_json::json!("val"),
    };
    assert_eq!(e.event_id(), Some("st1"));
}

#[test]
fn test_delta_events_have_no_event_id() {
    let e = Event::ModelMessageDelta {
        id: String::new(),
        text: "hi".into(),
    };
    assert_eq!(e.event_id(), Some(""));

    let e = Event::ModelThoughtDelta {
        id: String::new(),
        text: "think".into(),
    };
    assert_eq!(e.event_id(), Some(""));
}

// ── with_id ──

#[test]
fn test_with_id_sets_id_on_user_message() {
    let e = Event::UserMessage {
        id: String::new(),
        content: vec![ContentBlock::text("hi")],
    };
    let e = e.with_id("assigned_id".into());
    assert_eq!(e.event_id(), Some("assigned_id"));
    match &e {
        Event::UserMessage { content, .. } => assert!(!content.is_empty()),
        _ => panic!("变体不应改变"),
    }
}

#[test]
fn test_with_id_sets_id_on_model_message() {
    let e = Event::ModelMessage {
        id: String::new(),
        content: vec![ContentBlock::text("hi")],
    };
    let e = e.with_id("mid".into());
    assert_eq!(e.event_id(), Some("mid"));
}

#[test]
fn test_with_id_sets_id_on_model_thought() {
    let e = Event::ModelThought {
        id: String::new(),
        text: "think".into(),
    };
    let e = e.with_id("tid".into());
    assert_eq!(e.event_id(), Some("tid"));
}

#[test]
fn test_with_id_sets_id_on_function_call() {
    let e = Event::FunctionCall {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        args: serde_json::json!({}),
    };
    let e = e.with_id("fid".into());
    assert_eq!(e.event_id(), Some("fid"));
    match &e {
        Event::FunctionCall { call_id, name, .. } => {
            assert_eq!(call_id, "c1");
            assert_eq!(name, "bash");
        }
        _ => panic!("变体不应改变"),
    }
}

#[test]
fn test_with_id_sets_id_on_function_result() {
    let e = Event::FunctionResult {
        id: String::new(),
        call_id: "c1".into(),
        name: "bash".into(),
        content: None,
        code: None,
    };
    let e = e.with_id("frid".into());
    assert_eq!(e.event_id(), Some("frid"));
}

#[test]
fn test_with_id_sets_id_on_turn_stop() {
    let e = Event::TurnStop {
        id: String::new(),
        stop_reason: StopReason::EndTurn,
        token_usage: None,
    };
    let e = e.with_id("tsid".into());
    assert_eq!(e.event_id(), Some("tsid"));
    match &e {
        Event::TurnStop { stop_reason, .. } => assert_eq!(*stop_reason, StopReason::EndTurn),
        _ => panic!("变体不应改变"),
    }
}

#[test]
fn test_with_id_sets_id_on_state_update() {
    let e = Event::StateUpdate {
        id: String::new(),
        name: "k".into(),
        data: serde_json::json!("v"),
    };
    let e = e.with_id("stid".into());
    assert_eq!(e.event_id(), Some("stid"));
    match &e {
        Event::StateUpdate { name, data, .. } => {
            assert_eq!(name, "k");
            assert_eq!(data.as_str(), Some("v"));
        }
        _ => panic!("变体不应改变"),
    }
}

#[test]
fn test_with_id_preserves_delta_events() {
    let e = Event::ModelMessageDelta {
        id: String::new(),
        text: "partial".into(),
    };
    let e = e.with_id("ignored".into());
    assert_eq!(e.event_id(), Some("ignored"));
    match &e {
        Event::ModelMessageDelta { text, .. } => assert_eq!(text, "partial"),
        _ => panic!("变体不应改变"),
    }
}
