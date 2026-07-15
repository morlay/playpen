use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_content::{ContentBlock, Event, Resource};
use playpen_toolkit::terminal::{Command, CommandOutput, Terminal};

use crate::tool::{Tool, ToolContext};

pub(crate) struct BashTool {
    pub(crate) term: Arc<dyn Terminal>,
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "执行 shell 命令。支持可选的超时控制和工作目录。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(Command)).unwrap())
    }

    async fn execute(&self, ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let mut cmd: Command = serde_json::from_value(args)?;
        cmd.cancel_token = Some(ctx.cancellation_token().clone());

        let event_id = ctx.event_id().to_string();
        let call_id = ctx.call_id().to_string();
        let call_name = ctx.call_name().to_string();
        let mut rx = self.term.exec(cmd)?;

        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();
        let mut exit_code: Option<i32> = None;
        let mut cancelled = false;

        while let Some(item) = rx.recv().await {
            if ctx.cancellation_token().is_cancelled() {
                cancelled = true;
                break;
            }
            match item {
                CommandOutput::Stdout { text } => {
                    stdout_buf.push_str(&text);
                    ctx.send(Event::FunctionOutputDelta {
                        id: event_id.clone(),
                        call_id: call_id.clone(),
                        name: call_name.clone(),
                        text: text.to_string(),
                    });
                }
                CommandOutput::Stderr { text } => {
                    stderr_buf.push_str(&text);
                    ctx.send(Event::FunctionOutputDelta {
                        id: event_id.clone(),
                        call_id: call_id.clone(),
                        name: call_name.clone(),
                        text: text.to_string(),
                    });
                }
                CommandOutput::Exited { code } => {
                    exit_code = Some(code);
                    break;
                }
                CommandOutput::Cancelled => {
                    cancelled = true;
                    break;
                }
                CommandOutput::SpawnFailed { message } => {
                    anyhow::bail!("命令启动失败: {message}");
                }
            }
        }

        // rx.recv() 返回 None（channel 关闭）但未收到 Exited 事件时，默认视为 exit 0
        if exit_code.is_none() && !cancelled {
            exit_code = Some(0);
        }

        let mut blocks: Vec<ContentBlock> = Vec::new();

        if !stdout_buf.is_empty() {
            blocks.push(ContentBlock::resource(Resource::text(
                "/dev/stdout",
                "text/plain",
                &stdout_buf,
            )));
        }

        if !stderr_buf.is_empty() {
            blocks.push(ContentBlock::resource(Resource::text(
                "/dev/stderr",
                "text/plain",
                &stderr_buf,
            )));
        }

        if cancelled {
            blocks.push(ContentBlock::text("命令执行已被取消"));
        } else if blocks.is_empty() {
            blocks.push(ContentBlock::text("命令执行成功（无输出）"));
        }

        if let Some(code) = exit_code
            && let Some(last) = blocks.last_mut()
        {
            *last = last
                .clone()
                .with_annotations(serde_json::json!({ "exit_code": code }));
        }

        Ok(blocks)
    }
}

#[cfg(test)]
#[path = "bash_test.rs"]
mod tests;
