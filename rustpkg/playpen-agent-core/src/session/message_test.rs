use super::*;

#[test]
fn message_system_serde() {
    let msg = Message::system("你是一个助手");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"system\""));
    assert!(json.contains("你是一个助手"));
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text_content(), Some("你是一个助手"));
}

#[test]
fn message_user_serde() {
    let msg = Message::user("你好");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"user\""));
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text_content(), Some("你好"));
}

#[test]
fn message_assistant_serde() {
    let msg = Message::assistant("这是回复");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"assistant\""));
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text_content(), Some("这是回复"));
}

#[test]
fn message_tool_result_serde() {
    let msg = Message::tool("call_1", "工具结果");
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"role\":\"tool_result\""));
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text_content(), Some("工具结果"));
}

#[test]
fn content_block_tool_call_serde() {
    let tool_call = ContentBlock::ToolCall {
        id: "call_1".into(),
        name: "read".into(),
        arguments: serde_json::json!({"path": "/a.txt"}),
    };
    let json = serde_json::to_string(&tool_call).unwrap();
    assert!(json.contains("\"type\":\"tool_call\""));
    let back: ContentBlock = serde_json::from_str(&json).unwrap();
    match back {
        ContentBlock::ToolCall { id, name, arguments } => {
            assert_eq!(id, "call_1");
            assert_eq!(name, "read");
            assert_eq!(arguments["path"], "/a.txt");
        }
        _ => panic!("expected ToolCall"),
    }
}

#[test]
fn message_roundtrip_all_roles() {
    let msgs = vec![
        Message::system("system"),
        Message::user("user"),
        Message::assistant("assistant"),
        Message::tool("id", "result"),
    ];
    let json = serde_json::to_string(&msgs).unwrap();
    let back: Vec<Message> = serde_json::from_str(&json).unwrap();
    assert_eq!(back.len(), 4);
    assert_eq!(back[0].text_content(), Some("system"));
    assert_eq!(back[1].text_content(), Some("user"));
    assert_eq!(back[2].text_content(), Some("assistant"));
    assert_eq!(back[3].text_content(), Some("result"));
}
