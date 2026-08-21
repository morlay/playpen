//! 子代理（subagent）能力：`SubagentHost` 接缝 + `RunnerSubagentHost` 生产实现。
//!
//! 设计要点（对齐 zed 的 `ThreadEnvironment::create_subagent/resume_subagent` + `SubagentHandle::send`）：
//! - `SubagentHost::spawn` 合并 create/resume：`session_id: None` 新建（继承父 profile），`Some` 恢复既有会话。
//! - `SubagentHost::send` 发送 prompt 并等待子代理完成，收集最终 ModelMessage 文本。
//! - 结果索引 `message_start_index/message_end_index` 采用「可见条目」计数（仅
//!   UserMessage / ModelMessage / ModelThought / FunctionCall 四类事件），与 Zed 的
//!   `AgentThreadEntry` 语义对齐，供 client 在子代理会话中定位本轮 turn 的输出区间。
//! - 嵌套支持：`RunnerSubagentHost` 持 `AgentRunnerBuilder`，spawn 出的子 runner 用同一
//!   builder 重建自身 host（绑定子 runner 自己的 profile），链条不断。

use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use playpen_content::{ContentBlock, Event, StopReason};
use playpen_profile::AgentProfile;
use playpen_session::Session;
use tokio_util::sync::CancellationToken;

use crate::runner::{AgentRunner, AgentRunnerBuilder};

/// ACP meta key：子代理会话信息（Zed client 侧契约，见 docs/zed-acp-meta.md §3）。
pub const SUBAGENT_SESSION_INFO_META_KEY: &str = "subagent_session_info";

/// 子代理会话句柄。`session_id` 与 `message_start_index` 公开供工具层构造 ACP meta；
/// runner 私有持有，`SubagentHost::send` 内部使用。
pub struct SubagentHandle {
    pub session_id: String,
    /// send 前的可见条目数（本轮 turn 的起点索引）。
    pub message_start_index: usize,
    runner: Box<dyn AgentRunner>,
}

impl SubagentHandle {
    pub(crate) fn new(runner: Box<dyn AgentRunner>, message_start_index: usize) -> Self {
        let session_id = runner.id().to_string();
        Self {
            session_id,
            message_start_index,
            runner,
        }
    }
}

/// 一次子代理运行的输出。
#[derive(Debug)]
pub struct SubagentOutput {
    /// 子代理最终 ModelMessage 的文本。
    pub text: String,
    /// 完成后可见条目总数 - 1（本轮 turn 的终点索引，inclusive）。
    pub message_end_index: usize,
}

/// 子代理宿主接缝。工具（`SpawnAgentTool`）与测试共同穿过此接口。
#[async_trait]
pub trait SubagentHost: Send + Sync {
    /// 创建（`session_id: None`，继承父 profile）或恢复（`Some`）子代理会话。
    async fn spawn(&self, label: &str, session_id: Option<&str>) -> anyhow::Result<SubagentHandle>;

    /// 向子代理发送 prompt 并等待其完成，返回最终输出。
    /// 运行失败（LLM 错误 / 取消 / 拒绝）走 `Err`，但句柄中的 `session_id` 依然有效。
    async fn send(&self, handle: &SubagentHandle, prompt: &str) -> anyhow::Result<SubagentOutput>;
}

/// 基于 `AgentRunnerBuilder` 的生产实现：子代理是独立的持久化 session（`builder.create/resume`），
/// 与主 session 同级，天然支持跨 prompt 恢复。
#[derive(Clone)]
pub struct RunnerSubagentHost {
    builder: Arc<dyn AgentRunnerBuilder>,
    profile: Arc<dyn AgentProfile>,
    /// 父 runner 的取消令牌：父会话 cancel 时级联取消子代理。
    parent_cancel: CancellationToken,
}

impl RunnerSubagentHost {
    pub fn new(
        builder: Arc<dyn AgentRunnerBuilder>,
        profile: Arc<dyn AgentProfile>,
        parent_cancel: CancellationToken,
    ) -> Self {
        Self {
            builder,
            profile,
            parent_cancel,
        }
    }
}

#[async_trait]
impl SubagentHost for RunnerSubagentHost {
    async fn spawn(&self, label: &str, session_id: Option<&str>) -> anyhow::Result<SubagentHandle> {
        let mode = if session_id.is_some() {
            "resume"
        } else {
            "create"
        };
        tracing::info!(
            label,
            session_id = ?session_id,
            mode,
            "subagent spawn 开始"
        );

        let runner = match session_id {
            Some(sid) => self.builder.resume(sid).await?,
            None => {
                // Arc<dyn AgentProfile> → Box<dyn AgentProfile>：with_model_profile 重建自身
                // （与 apply_pending_config 的既有用法一致）。
                let profile = self.profile.with_model_profile(&|mp| mp.clone());
                self.builder.create(profile).await?
            }
        };
        // 嵌套传播：子 runner 用同一 builder 重建 host（绑定子 runner 自己的 profile）。
        let runner = runner.with_subagent_host(self.builder.clone());
        let message_start_index = entry_count(runner.session()).await;
        let sid = runner.id().to_string();

        tracing::info!(
            label,
            session_id = %sid,
            mode,
            profile = %runner.profile().name(),
            working_dir = %runner.profile().working_dir().display(),
            model = %runner.profile().model_profile().model,
            message_start_index,
            "subagent spawn 完成"
        );

        Ok(SubagentHandle::new(runner, message_start_index))
    }

    async fn send(&self, handle: &SubagentHandle, prompt: &str) -> anyhow::Result<SubagentOutput> {
        tracing::info!(
            session_id = %handle.session_id,
            prompt_len = prompt.len(),
            "subagent send 开始"
        );

        let stream = handle.runner.run(vec![ContentBlock::text(prompt)]).await;

        let mut output = String::new();
        let mut error: Option<String> = None;
        // 最后一次工具输出的文本（子代理无 ModelMessage 时回退给主 agent）
        let mut last_tool_output = String::new();

        tokio::pin!(stream);
        loop {
            tokio::select! {
                // 父会话已取消 → 级联取消子代理（父 cancel 必须终止子代理，避免空转）
                _ = self.parent_cancel.cancelled() => {
                    handle.runner.cancel().await;
                    error = Some("父会话已取消，子代理随之取消".into());
                    break;
                }
                event = stream.next() => {
                    match event {
                        Some(Event::ModelMessage { content, .. }) => {
                            output = content
                                .iter()
                                .filter_map(|b| match b {
                                    ContentBlock::Text(t) => Some(t.text.clone()),
                                    _ => None,
                                })
                                .collect();
                            tracing::debug!(
                                session_id = %handle.session_id,
                                output_len = output.len(),
                                "subagent 收到模型文本"
                            );
                        }
                        Some(Event::ModelThought { text, .. }) => {
                            tracing::debug!(
                                session_id = %handle.session_id,
                                thought_len = text.len(),
                                "subagent 收到思考"
                            );
                        }
                        Some(Event::FunctionCall { name, args, .. }) => {
                            tracing::info!(
                                session_id = %handle.session_id,
                                tool = %name,
                                args = %args,
                                "subagent 调用工具"
                            );
                        }
                        Some(Event::FunctionResult { name, content, code, .. }) => {
                            // 记录最后一次工具输出文本（供无 ModelMessage 时回退）
                            if let Some(ref blocks) = content {
                                let text: String = blocks
                                    .iter()
                                    .filter_map(|b| match b {
                                        ContentBlock::Text(t) => Some(t.text.clone()),
                                        _ => None,
                                    })
                                    .collect();
                                if !text.trim().is_empty() {
                                    last_tool_output = text;
                                }
                            }
                            tracing::debug!(
                                session_id = %handle.session_id,
                                tool = %name,
                                code,
                                result_len = content.as_ref().map(|b| b.len()).unwrap_or(0),
                                "subagent 工具结果"
                            );
                        }
                        Some(Event::TurnStop { stop_reason, .. }) => {
                            error = match &stop_reason {
                                StopReason::Error(e) => Some(e.clone()),
                                StopReason::Cancelled => Some("子代理被取消".into()),
                                StopReason::Refusal => Some("子代理拒绝处理该消息".into()),
                                _ => None,
                            };
                            tracing::info!(
                                session_id = %handle.session_id,
                                ?stop_reason,
                                "subagent turn 结束"
                            );
                            // 不 break：带 tool_call 的中间 TurnStop 之后还有 FunctionResult 与
                            // 下一轮循环；只有流结束（None）才是真正结束。
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
            }
        }

        let message_end_index = entry_count(handle.runner.session()).await.saturating_sub(1);

        match error {
            Some(e) => {
                tracing::warn!(
                    session_id = %handle.session_id,
                    error = %e,
                    "subagent send 失败"
                );
                Err(anyhow::anyhow!(e))
            }
            None => {
                if output.trim().is_empty() {
                    if !last_tool_output.trim().is_empty() {
                        // 常见场景：子代理只执行工具调用、未输出文本消息（如思考等级高时模型
                        // 调完工具直接收尾）。回退到最后的工具输出，让主 agent 看到实际产出。
                        tracing::warn!(
                            session_id = %handle.session_id,
                            "subagent 无文本输出，回退最后工具输出"
                        );
                        output = format!(
                            "（子代理仅执行了工具调用，无文本总结。最后工具输出：\n{last_tool_output}）"
                        );
                    } else {
                        tracing::warn!(
                            session_id = %handle.session_id,
                            "subagent 完成但无文本输出（可能仅执行了工具调用）"
                        );
                        output = "（子代理已完成任务，未返回文本消息）".into();
                    }
                }
                tracing::info!(
                    session_id = %handle.session_id,
                    output_len = output.len(),
                    message_end_index,
                    "subagent send 完成"
                );
                Ok(SubagentOutput {
                    text: output,
                    message_end_index,
                })
            }
        }
    }
}

/// 可见条目计数：与 Zed `AgentThreadEntry` 对齐，仅计四类产生 UI 条目的事件。
/// StateUpdate / Model*Delta / FunctionOutputDelta / FunctionResult / TurnStop 不计数。
pub(crate) async fn entry_count(session: &dyn Session) -> usize {
    session
        .events()
        .all()
        .await
        .fold(0usize, |acc, e| async move {
            acc + usize::from(matches!(
                e,
                Event::UserMessage { .. }
                    | Event::ModelMessage { .. }
                    | Event::ModelThought { .. }
                    | Event::FunctionCall { .. }
            ))
        })
        .await
}

#[cfg(test)]
#[path = "subagent_test.rs"]
mod tests;
