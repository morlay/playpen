//! AgentEvent → ACP SessionUpdate / StopReason 映射。

use agent_client_protocol::schema::{
    ContentBlock, ContentChunk, SessionUpdate, StopReason, TextContent, ToolCall, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use playpen_agent_core::agent::runner::AgentEvent;

/// 将 playpen 工具名映射为 ACP ToolKind。
pub fn map_tool_kind(name: &str) -> ToolKind {
    match name {
        "read" => ToolKind::Read,
        "grep" | "find" => ToolKind::Search,
        "edit" | "write" => ToolKind::Edit,
        "move" => ToolKind::Move,
        "bash" => ToolKind::Execute,
        "webfetch" => ToolKind::Fetch,
        _ => ToolKind::Other,
    }
}

/// 将 playpen StopReason 映射为 ACP StopReason。
pub fn map_stop_reason(sr: &playpen_agent_core::model::StopReason) -> StopReason {
    match sr {
        playpen_agent_core::model::StopReason::Stop
        | playpen_agent_core::model::StopReason::ToolUse => StopReason::EndTurn,
        playpen_agent_core::model::StopReason::Length => StopReason::MaxTokens,
        playpen_agent_core::model::StopReason::Aborted => StopReason::Cancelled,
        playpen_agent_core::model::StopReason::Error => StopReason::Refusal,
    }
}

/// 将单个 AgentEvent 转换为 Option<SessionUpdate>。
///
/// `Done` / `Error` 返回 None，由 prompt handler 特殊处理。
/// `ToolCallStart` 返回 ToolCall(Pending)，prompt handler 负责背靠背发送 ToolCallUpdate(InProgress)。
pub fn to_session_update(event: &AgentEvent) -> Option<SessionUpdate> {
    match event {
        AgentEvent::TextDelta(text) => {
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text.clone())));
            Some(SessionUpdate::AgentMessageChunk(chunk))
        }
        AgentEvent::ReasoningDelta(text) => {
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text.clone())));
            Some(SessionUpdate::AgentThoughtChunk(chunk))
        }
        AgentEvent::ToolCallStart {
            id,
            name,
            arguments,
        } => {
            let raw_input: serde_json::Value =
                serde_json::from_str(arguments).unwrap_or(serde_json::Value::String(arguments.clone()));
            let tc = ToolCall::new(id.clone(), name.clone())
                .kind(map_tool_kind(name))
                .status(ToolCallStatus::Pending)
                .raw_input(raw_input);
            Some(SessionUpdate::ToolCall(tc))
        }
        AgentEvent::ToolCallResult { id, result } => {
            let raw_output: serde_json::Value =
                serde_json::from_str(result).unwrap_or(serde_json::Value::String(result.clone()));
            let fields = ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(raw_output);
            let update = ToolCallUpdate::new(id.clone(), fields);
            Some(SessionUpdate::ToolCallUpdate(update))
        }
        AgentEvent::Done { .. } | AgentEvent::Error(_) => None,
    }
}

/// 为 ToolCallStart 构建背靠背的 ToolCallUpdate(InProgress)。
/// prompt handler 在发送 ToolCall(Pending) 后立即发送此更新。
pub fn to_tool_call_in_progress(event: &AgentEvent) -> Option<SessionUpdate> {
    match event {
        AgentEvent::ToolCallStart { id, .. } => {
            let fields = ToolCallUpdateFields::new().status(ToolCallStatus::InProgress);
            let update = ToolCallUpdate::new(id.clone(), fields);
            Some(SessionUpdate::ToolCallUpdate(update))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_tool_kind_all_known() {
        assert_eq!(map_tool_kind("read"), ToolKind::Read);
        assert_eq!(map_tool_kind("grep"), ToolKind::Search);
        assert_eq!(map_tool_kind("find"), ToolKind::Search);
        assert_eq!(map_tool_kind("edit"), ToolKind::Edit);
        assert_eq!(map_tool_kind("write"), ToolKind::Edit);
        assert_eq!(map_tool_kind("move"), ToolKind::Move);
        assert_eq!(map_tool_kind("bash"), ToolKind::Execute);
        assert_eq!(map_tool_kind("webfetch"), ToolKind::Fetch);
    }

    #[test]
    fn test_map_tool_kind_unknown() {
        assert_eq!(map_tool_kind("unknown_tool"), ToolKind::Other);
    }

    #[test]
    fn test_map_stop_reason() {
        use playpen_agent_core::model::StopReason as SR;
        assert_eq!(map_stop_reason(&SR::Stop), StopReason::EndTurn);
        assert_eq!(map_stop_reason(&SR::ToolUse), StopReason::EndTurn);
        assert_eq!(map_stop_reason(&SR::Length), StopReason::MaxTokens);
        assert_eq!(map_stop_reason(&SR::Aborted), StopReason::Cancelled);
        assert_eq!(map_stop_reason(&SR::Error), StopReason::Refusal);
    }

    #[test]
    fn test_text_delta_to_update() {
        let event = AgentEvent::TextDelta("hello".into());
        let update = to_session_update(&event).unwrap();
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
                ContentBlock::Text(tc) => assert_eq!(tc.text, "hello"),
                _ => panic!("应为 Text"),
            },
            _ => panic!("应为 AgentMessageChunk"),
        }
    }

    #[test]
    fn test_reasoning_delta_to_update() {
        let event = AgentEvent::ReasoningDelta("thinking...".into());
        let update = to_session_update(&event).unwrap();
        match update {
            SessionUpdate::AgentThoughtChunk(_) => {}
            _ => panic!("应为 AgentThoughtChunk"),
        }
    }

    #[test]
    fn test_tool_call_start_is_pending() {
        let event = AgentEvent::ToolCallStart {
            id: "tc1".into(),
            name: "bash".into(),
            arguments: r#"{"cmd":"ls"}"#.into(),
        };
        let update = to_session_update(&event).unwrap();
        match update {
            SessionUpdate::ToolCall(tc) => {
                assert_eq!(tc.tool_call_id.to_string(), "tc1");
                assert_eq!(tc.title, "bash");
                assert_eq!(tc.kind, ToolKind::Execute);
                assert_eq!(tc.status, ToolCallStatus::Pending); // ← ACP 规范：初始为 Pending
            }
            _ => panic!("应为 ToolCall"),
        }
    }

    #[test]
    fn test_tool_call_in_progress_update() {
        let event = AgentEvent::ToolCallStart {
            id: "tc2".into(),
            name: "read".into(),
            arguments: r#"{"path":"/tmp"}"#.into(),
        };
        let update = to_tool_call_in_progress(&event).unwrap();
        match update {
            SessionUpdate::ToolCallUpdate(tu) => {
                assert_eq!(tu.tool_call_id.to_string(), "tc2");
                assert_eq!(tu.fields.status, Some(ToolCallStatus::InProgress));
            }
            _ => panic!("应为 ToolCallUpdate"),
        }
    }

    #[test]
    fn test_tool_call_result_is_completed() {
        let event = AgentEvent::ToolCallResult {
            id: "tc3".into(),
            result: "output".into(),
        };
        let update = to_session_update(&event).unwrap();
        match update {
            SessionUpdate::ToolCallUpdate(tu) => {
                assert_eq!(tu.tool_call_id.to_string(), "tc3");
                assert_eq!(tu.fields.status, Some(ToolCallStatus::Completed));
            }
            _ => panic!("应为 ToolCallUpdate"),
        }
    }

    #[test]
    fn test_done_and_error_are_none() {
        assert!(to_session_update(&AgentEvent::Done {
            message: playpen_agent_core::session::message::Message::user(""),
            usage: Default::default(),
            stop_reason: playpen_agent_core::model::StopReason::Stop,
        })
        .is_none());
        assert!(to_session_update(&AgentEvent::Error("err".into())).is_none());
    }
}
