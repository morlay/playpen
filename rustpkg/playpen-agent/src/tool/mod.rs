use std::sync::Arc;

use async_trait::async_trait;
use rig_core::completion::ToolDefinition;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use playpen_content::Event;
use playpen_toolkit::Toolkit;

mod bash;
mod edit;
mod find;
mod grep;
mod r#move;
mod read;
mod webfetch;
mod write;

pub(crate) use bash::BashTool;
pub(crate) use edit::EditFileTool;
pub(crate) use find::FindTool;
pub(crate) use grep::GrepTool;
pub(crate) use r#move::MoveFileTool;
pub(crate) use read::ReadFileTool;
pub(crate) use webfetch::WebFetchTool;
pub(crate) use write::WriteFileTool;

/// 工具调用上下文。
/// 携带完整的 FunctionCall 信息，供工具执行时构造 FunctionOutputDelta / FunctionResult 使用。
pub struct ToolContext {
    /// FunctionCall 的 id（event_id）
    event_id: String,
    /// FunctionCall 的 call_id（LLM 分配的 tool_call_id）
    call_id: String,
    /// FunctionCall 的 name
    call_name: String,
    tx: mpsc::UnboundedSender<Event>,
    cancel: CancellationToken,
}

impl ToolContext {
    pub fn new(
        event_id: impl Into<String>,
        call_id: impl Into<String>,
        call_name: impl Into<String>,
        tx: mpsc::UnboundedSender<Event>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            event_id: event_id.into(),
            call_id: call_id.into(),
            call_name: call_name.into(),
            tx,
            cancel,
        }
    }

    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    pub fn call_name(&self) -> &str {
        &self.call_name
    }

    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// 工具执行时发射事件（如 FunctionOutputDelta）。
    pub fn send(&self, event: Event) {
        let _ = self.tx.send(event);
    }
}

/// 工具 trait（无 adk 依赖）。
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Option<serde_json::Value>;
    async fn execute(
        &self,
        ctx: ToolContext,
        args: serde_json::Value,
    ) -> anyhow::Result<Vec<playpen_content::ContentBlock>>;
}

/// 将我们的工具列表转为 rig ToolDefinition 列表（用于注入 LLM 请求）。
pub fn to_tool_definitions(tools: &[Arc<dyn Tool>]) -> Vec<ToolDefinition> {
    tools
        .iter()
        .map(|t| ToolDefinition {
            name: t.name().to_string(),
            description: t.description().to_string(),
            parameters: t.parameters_schema().unwrap_or(serde_json::json!({})),
        })
        .collect()
}

/// 从 Toolkit 构建工具列表。
pub fn into_tools(toolkit: &Toolkit) -> Vec<Arc<dyn Tool>> {
    let fs = toolkit.file_system.clone();
    let term = toolkit.terminal.clone();
    let web = toolkit.fetcher.clone();

    vec![
        Arc::new(ReadFileTool { fs: fs.clone() }),
        Arc::new(EditFileTool { fs: fs.clone() }),
        Arc::new(WriteFileTool { fs: fs.clone() }),
        Arc::new(GrepTool { fs: fs.clone() }),
        Arc::new(FindTool { fs: fs.clone() }),
        Arc::new(MoveFileTool { fs }),
        Arc::new(WebFetchTool { web }),
        Arc::new(BashTool { term }),
    ]
}
