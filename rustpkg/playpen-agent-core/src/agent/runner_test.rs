use super::*;

#[test]
fn agent_event_text_serde() {
    let event = AgentEvent::TextDelta("hello".into());
    let json = serde_json::to_string(&event).unwrap();
    let back: AgentEvent = serde_json::from_str(&json).unwrap();
    match back {
        AgentEvent::TextDelta(t) => assert_eq!(t, "hello"),
        _ => panic!("expected TextDelta"),
    }
}

#[test]
fn agent_event_tool_call_serde() {
    let event = AgentEvent::ToolCallStart {
        id: "call_1".into(),
        name: "read".into(),
        arguments: r#"{"path":"/a.txt"}"#.into(),
    };
    let json = serde_json::to_string(&event).unwrap();
    let back: AgentEvent = serde_json::from_str(&json).unwrap();
    match back {
        AgentEvent::ToolCallStart { id, name, arguments } => {
            assert_eq!(id, "call_1");
            assert_eq!(name, "read");
            assert!(arguments.contains("/a.txt"));
        }
        _ => panic!("expected ToolCallStart"),
    }
}

#[test]
fn agent_event_done_serde() {
    let event = AgentEvent::Done {
        message: crate::session::message::Message::assistant("done"),
        usage: Default::default(),
        stop_reason: crate::model::StopReason::Stop,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"role\":\"assistant\""));
    let back: AgentEvent = serde_json::from_str(&json).unwrap();
    assert!(matches!(back, AgentEvent::Done { .. }));
}

#[test]
fn agent_event_error_serde() {
    let event = AgentEvent::Error("something wrong".into());
    let json = serde_json::to_string(&event).unwrap();
    let back: AgentEvent = serde_json::from_str(&json).unwrap();
    match back {
        AgentEvent::Error(msg) => assert_eq!(msg, "something wrong"),
        _ => panic!("expected Error"),
    }
}

#[test]
fn stream_state_new() {
    let state = StreamState::new("gpt-4".into());
    assert_eq!(state.usage.total, 0);
    assert!(state.text.is_empty());
}
