use std::path::Path;

use agent_client_protocol::schema::v1::{
    Content as AcpContent, ContentBlock as AcpContentBlock, ContentChunk, Diff, SessionUpdate,
    Terminal, TextContent as AcpTextContent, ToolCall, ToolCallContent, ToolCallStatus,
    ToolCallUpdate, ToolCallUpdateFields,
};
use playpen_config::model::Model;
use playpen_content::ContentBlock;
use playpen_content::Event;

use crate::acp_content::{map_turn_stop, text_from_opt_content, to_acp_blocks};
use crate::display::{build_tool_title, extract_cwd, map_tool_kind, meta_with_tool_name};

/// 聚合事件映射所需的上下文，避免函数入参扩散。
/// 只有 `map_event` 对外公开，所有上下文通过此 struct 传递。
pub struct EventMapper<'a> {
    working_dir: &'a Path,
    term_enabled: bool,
    replay: bool,
    model: Option<&'a Model>,
    /// 当 event 本身没有 message_id 时使用的 fallback
    default_message_id: Option<&'a str>,
}

impl<'a> EventMapper<'a> {
    /// 仅 `project_root` 为必选，其余通过 `with_*` 链式设置，默认均为 `false`/`None`。
    pub fn new(working_dir: &'a Path) -> Self {
        Self {
            working_dir,
            term_enabled: false,
            replay: false,
            model: None,
            default_message_id: None,
        }
    }

    pub fn with_term_enabled(mut self, enabled: bool) -> Self {
        self.term_enabled = enabled;
        self
    }

    pub fn with_replay(mut self, replay: bool) -> Self {
        self.replay = replay;
        self
    }

    pub fn with_default_message_id(mut self, id: &'a str) -> Self {
        self.default_message_id = Some(id);
        self
    }

    pub fn with_model(mut self, model: Option<&'a Model>) -> Self {
        self.model = model;
        self
    }

    /// 将 `Event` 映射为 ACP `SessionUpdate` 序列。
    /// `message_id` 优先从 event 的 id 取，fallback 到 `with_default_message_id` 设置的值。
    pub fn map_event(&self, event: &Event) -> Vec<SessionUpdate> {
        let message_id = event
            .event_id()
            .or(self.default_message_id)
            .unwrap_or_default();

        if self.replay {
            return match event {
                Event::UserMessage { .. } => self.map_user_message(event, message_id),
                Event::ModelMessageDelta { .. } => vec![],
                Event::ModelMessage { .. } => self.map_model_message(event, message_id),
                Event::ModelThoughtDelta { .. } => vec![],
                Event::ModelThought { .. } => self.map_model_thought(event, message_id),
                Event::FunctionCall { .. } => self.map_function(event),
                Event::FunctionOutputDelta { .. } => vec![],
                Event::FunctionResult { .. } => self.map_function(event),
                Event::TurnStop {
                    stop_reason,
                    token_usage,
                    ..
                } => map_turn_stop(stop_reason, token_usage.as_ref(), self.model),
                Event::StateUpdate { .. } => vec![],
            };
        }

        match event {
            Event::UserMessage { .. } => vec![],
            Event::ModelMessageDelta { .. } => self.map_model_message(event, message_id),
            Event::ModelMessage { .. } => vec![],
            Event::ModelThoughtDelta { .. } => self.map_model_thought(event, message_id),
            Event::ModelThought { .. } => vec![],
            Event::FunctionCall { .. }
            | Event::FunctionOutputDelta { .. }
            | Event::FunctionResult { .. } => self.map_function(event),
            Event::TurnStop {
                stop_reason,
                token_usage,
                ..
            } => map_turn_stop(stop_reason, token_usage.as_ref(), self.model),
            Event::StateUpdate { .. } => vec![],
        }
    }

    fn map_model_message(&self, event: &Event, message_id: &str) -> Vec<SessionUpdate> {
        match event {
            Event::ModelMessageDelta { text, .. } => {
                if text.is_empty() {
                    return vec![];
                }
                vec![SessionUpdate::AgentMessageChunk(
                    ContentChunk::new(AcpContentBlock::Text(AcpTextContent::new(text.clone())))
                        .message_id(message_id),
                )]
            }
            Event::ModelMessage { content, .. } => to_acp_blocks(content)
                .into_iter()
                .map(|block| {
                    SessionUpdate::AgentMessageChunk(
                        ContentChunk::new(block).message_id(message_id),
                    )
                })
                .collect(),
            _ => vec![],
        }
    }

    fn map_model_thought(&self, event: &Event, message_id: &str) -> Vec<SessionUpdate> {
        let text = match event {
            Event::ModelThoughtDelta { text, .. } => text,
            Event::ModelThought { text, .. } => text,
            _ => return vec![],
        };
        if text.is_empty() {
            return vec![];
        }
        vec![SessionUpdate::AgentThoughtChunk(
            ContentChunk::new(AcpContentBlock::Text(AcpTextContent::new(text.clone())))
                .message_id(message_id),
        )]
    }

    fn map_function(&self, event: &Event) -> Vec<SessionUpdate> {
        match event {
            Event::FunctionCall {
                call_id: id,
                name,
                args,
                ..
            } => self.map_function_call(id, name, args),
            Event::FunctionOutputDelta {
                call_id: id,
                name,
                text,
                ..
            } => self.map_function_delta(id, name, text),
            Event::FunctionResult {
                call_id: id,
                name,
                content,
                ..
            } => self.map_function_result(id, name, content),
            _ => vec![],
        }
    }

    fn map_function_call(
        &self,
        id: &str,
        name: &str,
        args: &serde_json::Value,
    ) -> Vec<SessionUpdate> {
        let args_map = args.as_object().cloned().unwrap_or_default();
        let is_bash = name == "bash" && self.term_enabled;

        if is_bash {
            let cwd = extract_cwd(&args_map, self.working_dir);
            let title = build_tool_title(name, &args_map, self.working_dir);
            let meta = agent_client_protocol::schema::v1::Meta::from_iter([(
                "terminal_info".into(),
                serde_json::json!({"terminal_id": id, "cwd": cwd}),
            )]);
            let tc = ToolCall::new(id.to_string(), title)
                .kind(agent_client_protocol::schema::v1::ToolKind::Other)
                .status(ToolCallStatus::InProgress)
                .content(vec![ToolCallContent::Terminal(Terminal::new(
                    id.to_string(),
                ))])
                .meta(Some(meta));
            vec![SessionUpdate::ToolCall(tc)]
        } else {
            let title = build_tool_title(name, &args_map, self.working_dir);
            vec![
                SessionUpdate::ToolCall(
                    ToolCall::new(id.to_string(), title)
                        .kind(map_tool_kind(name))
                        .status(ToolCallStatus::Pending)
                        .raw_input(args.clone())
                        .meta(Some(meta_with_tool_name(name))),
                ),
                SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                    id.to_string(),
                    ToolCallUpdateFields::new().status(ToolCallStatus::InProgress),
                )),
            ]
        }
    }

    fn map_function_delta(&self, id: &str, name: &str, text: &str) -> Vec<SessionUpdate> {
        if name == "bash" && self.term_enabled {
            let meta = agent_client_protocol::schema::v1::Meta::from_iter([(
                "terminal_output".into(),
                serde_json::json!({"terminal_id": id, "data": text}),
            )]);
            vec![SessionUpdate::ToolCallUpdate(
                ToolCallUpdate::new(id.to_string(), ToolCallUpdateFields::new()).meta(Some(meta)),
            )]
        } else {
            vec![SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                id.to_string(),
                ToolCallUpdateFields::new()
                    .status(ToolCallStatus::InProgress)
                    .content(Some(vec![ToolCallContent::Content(AcpContent::new(
                        AcpContentBlock::Text(AcpTextContent::new(text.to_string())),
                    ))])),
            ))]
        }
    }

    fn map_function_result(
        &self,
        id: &str,
        name: &str,
        content: &Option<Vec<ContentBlock>>,
    ) -> Vec<SessionUpdate> {
        let text = text_from_opt_content(content);

        // Extract annotations from the first TextContent block
        let annotations: Option<&serde_json::Value> = content.as_ref().and_then(|blocks| {
            blocks.iter().find_map(|b| match b {
                ContentBlock::Text(t) => t.annotations.as_ref(),
                _ => None,
            })
        });

        // Determine status from exit_code if available, fallback to text-based detection
        let status = annotations
            .and_then(|a| a.get("exit_code").and_then(|c| c.as_i64()))
            .map(|code| {
                if code == 0 {
                    ToolCallStatus::Completed
                } else {
                    ToolCallStatus::Failed
                }
            })
            .unwrap_or_else(|| {
                if text.starts_with("Error") || text.contains("沙箱执行失败") {
                    ToolCallStatus::Failed
                } else {
                    ToolCallStatus::Completed
                }
            });

        let mut updates = Vec::new();

        if name == "bash" && self.term_enabled {
            let exit_code = annotations
                .and_then(|a| a.get("exit_code").and_then(|c| c.as_i64()))
                .unwrap_or(0);

            // Replay: deltas were not persisted, reconstruct output from result text
            if self.replay && !text.is_empty() {
                let meta = agent_client_protocol::schema::v1::Meta::from_iter([(
                    "terminal_output".into(),
                    serde_json::json!({
                        "terminal_id": id,
                        "data": text,
                    }),
                )]);
                updates.push(SessionUpdate::ToolCallUpdate(
                    ToolCallUpdate::new(id.to_string(), ToolCallUpdateFields::new())
                        .meta(Some(meta)),
                ));
            }

            // Emit exit meta (both replay and live)
            let meta = agent_client_protocol::schema::v1::Meta::from_iter([(
                "terminal_exit".into(),
                serde_json::json!({"terminal_id": id, "exit_code": exit_code, "signal": null}),
            )]);
            updates.push(SessionUpdate::ToolCallUpdate(
                ToolCallUpdate::new(id.to_string(), ToolCallUpdateFields::new().status(status))
                    .meta(Some(meta)),
            ));
        } else {
            // Non-terminal tools: emit content with text and optional diffs
            let diff_content = annotations.and_then(|a| self.build_diff_content(a));

            // Resource content → map via to_acp_blocks (no wrap_code_block)
            // Text content → use directly as markdown
            let has_resource = content.as_ref().is_some_and(|blocks| {
                blocks
                    .iter()
                    .any(|b| matches!(b, ContentBlock::Resource(_)))
            });

            let mut content_parts: Vec<ToolCallContent> = if has_resource {
                if let Some(blocks) = content.as_deref() {
                    to_acp_blocks(blocks)
                        .into_iter()
                        .map(|block| ToolCallContent::Content(AcpContent::new(block)))
                        .collect()
                } else {
                    vec![]
                }
            } else {
                vec![ToolCallContent::Content(AcpContent::new(
                    AcpContentBlock::Text(AcpTextContent::new(text)),
                ))]
            };
            if let Some(diffs) = diff_content {
                content_parts.extend(diffs);
            }

            updates.push(SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                id.to_string(),
                ToolCallUpdateFields::new()
                    .status(status)
                    .content(Some(content_parts)),
            )));
        }
        updates
    }

    fn map_user_message(&self, event: &Event, message_id: &str) -> Vec<SessionUpdate> {
        let content = match event {
            Event::UserMessage { content, .. } => content,
            _ => return vec![],
        };
        to_acp_blocks(content)
            .into_iter()
            .map(|block| {
                SessionUpdate::UserMessageChunk(ContentChunk::new(block).message_id(message_id))
            })
            .collect()
    }

    fn build_diff_content(&self, annotations: &serde_json::Value) -> Option<Vec<ToolCallContent>> {
        let path = annotations.get("path")?.as_str()?;
        let abs = if Path::new(path).is_absolute() {
            Path::new(path).to_path_buf()
        } else {
            self.working_dir.join(path)
        };

        // edit tool: diffs array with old_text/new_text pairs
        if let Some(diffs) = annotations.get("diffs").and_then(|d| d.as_array()) {
            let items: Vec<_> = diffs
                .iter()
                .filter_map(|d| {
                    let ot = d.get("old_text")?.as_str()?;
                    let nt = d.get("new_text")?.as_str()?;
                    Some(ToolCallContent::Diff(
                        Diff::new(abs.clone(), nt).old_text(Some(ot.to_string())),
                    ))
                })
                .collect();
            if items.is_empty() { None } else { Some(items) }
        // write tool: new_text with optional old_text
        } else if let Some(nt) = annotations.get("new_text").and_then(|v| v.as_str()) {
            let ot = annotations
                .get("old_text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let mut diff = Diff::new(abs, nt);
            if let Some(o) = ot {
                diff = diff.old_text(Some(o));
            }
            Some(vec![ToolCallContent::Diff(diff)])
        } else {
            None
        }
    }
}

#[cfg(test)]
#[path = "event_mapper_test.rs"]
mod tests;
