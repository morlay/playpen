use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use playpen_content::ContentBlock;

use crate::subagent::{SUBAGENT_SESSION_INFO_META_KEY, SubagentHost};
use crate::tool::{Tool, ToolContext};

/// Spawn 子代理执行独立子任务。
///
/// ### 委托子任务的设计
/// - 子代理看不到你的对话历史，请把所需上下文（文件路径、要求、约束）完整写进 message。
/// - 子任务必须具体、边界清晰、自成一体，且实质推进主任务。
/// - 不要委托你一两步工具调用就能完成的事（如读一个文件返回内容）。
/// - 委托后专注协调与综合结果，不要重复子代理的工作。
/// - 对同一未决子问题避免重复委托，除非新任务确实不同且必要。
/// - 代码编辑类子任务请拆分为互不重叠的写集。
/// - 用返回的 session_id 继续追问同一子问题，而不是创建重复会话；续聊时只需简短直入的消息，
///   不要重复原始任务或上下文。
///
/// ### 并行委托
/// - 相互独立的调研类子任务可并行委托。
/// - 计划中互相独立的步骤优先并行委托而非串行。
///
/// ### 输出
/// - 只返回子代理的最终消息；成功时附带 session_id 供后续消息复用。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct SpawnAgentInput {
    /// 子代理运行时显示的短标签（如 "调研备选方案"）
    pub label: String,
    /// 子代理的任务消息。新会话需包含完整上下文；带 session_id 续聊时只需简短消息。
    pub message: String,
    /// 既有子代理会话 id（续聊），缺省时创建新会话。
    #[serde(default)]
    pub session_id: Option<String>,
}

/// 工具：生成子代理执行独立子任务。
pub struct SpawnAgentTool {
    host: Arc<dyn SubagentHost>,
}

impl SpawnAgentTool {
    pub fn new(host: Arc<dyn SubagentHost>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl Tool for SpawnAgentTool {
    fn name(&self) -> &str {
        "spawn_agent"
    }

    fn description(&self) -> &str {
        "生成一个子代理（独立的 agent 会话）执行定义明确的子任务，返回其最终输出与 session_id。子代理与主代理共享工作目录与模型配置，但看不到本会话历史。可将返回的 session_id 传入后续调用以继续同一子任务。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(SpawnAgentInput)).unwrap())
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let input: SpawnAgentInput = serde_json::from_value(args)?;
        tracing::info!(
            label = %input.label,
            has_session = input.session_id.is_some(),
            "spawn_agent 工具调用"
        );

        // 创建/恢复失败：无 session 信息可携带，交给 execute_tool_call 的 Err 通道。
        let handle = self
            .host
            .spawn(&input.label, input.session_id.as_deref())
            .await?;
        tracing::info!(session_id = %handle.session_id, "spawn_agent 子代理就绪");

        // 运行中提示（delta 只发射不持久化，与 bash 工具同款）
        ctx.send(playpen_content::Event::FunctionOutputDelta {
            id: ctx.event_id().to_string(),
            call_id: ctx.call_id().to_string(),
            name: ctx.call_name().to_string(),
            text: format!("子代理「{}」已就绪（session {}），正在运行…", input.label, handle.session_id),
        });

        let result = self.host.send(&handle, &input.message).await;
        let (text, code) = match &result {
            Ok(output) => (output.text.clone(), 0i64),
            Err(e) => (format!("子代理执行失败: {e}"), 1i64),
        };
        let message_end_index = result.as_ref().ok().map(|o| o.message_end_index);
        tracing::info!(
            session_id = %handle.session_id,
            is_ok = result.is_ok(),
            output_len = text.len(),
            "spawn_agent 子代理完成"
        );

        let session_info = serde_json::json!({
            "session_id": handle.session_id,
            "message_start_index": handle.message_start_index,
            "message_end_index": message_end_index,
        });

        // 铁律：成败都返回 Ok + annotations（Err 通道无 annotations，会丢失 session_id）。
        // `_meta.*` 前缀由 event_mapper 还原为 ACP ToolCallUpdate meta（复用既有约定）。
        Ok(vec![ContentBlock::text(text).with_annotations(serde_json::json!({
            format!("_meta.{SUBAGENT_SESSION_INFO_META_KEY}"): session_info,
            "exit_code": code,
        }))])
    }
}

#[cfg(test)]
#[path = "spawn_agent_test.rs"]
mod tests;
