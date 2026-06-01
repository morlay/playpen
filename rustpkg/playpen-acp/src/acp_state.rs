use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, Weak};

use agent_client_protocol::ConnectionTo;
use agent_client_protocol::role::acp::Client;
use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandsUpdate, SessionNotification, SessionUpdate, ToolCall,
    ToolCallStatus,
};
use async_trait::async_trait;
use playpen_agent::{AgentRunner, AgentRunnerBuilder};
use playpen_profile::Skill;

use crate::slash_command;

#[derive(Debug, Clone, Default)]
pub struct PendingConfig {
    pub profile_name: Option<String>,
    pub model_key: Option<String>,
    pub thought_level: Option<String>,
}

/// 仅在 prompt 执行期间持有 runner 的 Weak，供 cancel 查找；cx 由各 handler 通过参数传入。
/// runner 的强引用由 spawned task 持有，task 结束后自动释放，Weak 自然过期。
pub struct AcpState {
    pub builder: Box<dyn AgentRunnerBuilder>,
    pub running_runners: tokio::sync::RwLock<HashMap<String, Weak<Box<dyn AgentRunner>>>>,
    pub pending_configs: Mutex<HashMap<String, PendingConfig>>,
    pub terminal_output_enabled: AtomicBool,
}

/// 发送 SessionUpdate 通知，cx 由调用方传入，不存储在 AcpState 中。
pub(crate) fn send_notification(
    cx: &ConnectionTo<Client>,
    sid: &str,
    update: SessionUpdate,
) -> Result<(), agent_client_protocol::Error> {
    cx.send_notification(SessionNotification::new(sid.to_string(), update))
        .map_err(|e| {
            tracing::error!(error = %e, "发送通知失败");
            agent_client_protocol::util::internal_error(format!("发送通知失败: {e}"))
        })
}

/// 发送 AvailableCommands 通知。
pub(crate) fn send_available_commands(ctx: &dyn Context, sid: &str, skills: &[Box<dyn Skill>]) {
    let mut commands: Vec<AvailableCommand> = Vec::new();

    // 内置命令
    commands.push(slash_command::build_rewind_available_command());

    // skill 命令
    for s in skills {
        let meta = s.metadata();
        commands.push(slash_command::build_skill_available_command(
            &meta.name,
            &meta.description,
        ));
    }
    if !commands.is_empty() {
        let _ = ctx.notify_update(
            sid,
            SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
        );
    }
}

impl AcpState {
    pub fn new(builder: Box<dyn AgentRunnerBuilder>) -> Self {
        Self {
            builder,
            running_runners: tokio::sync::RwLock::new(HashMap::new()),
            pending_configs: Mutex::new(HashMap::new()),
            terminal_output_enabled: AtomicBool::new(false),
        }
    }

    pub fn set_pending_config(&self, sid: &str, config_id: &str, value: &str) {
        let mut map = self
            .pending_configs
            .lock()
            .expect("pending_configs poisoned");
        let entry = map.entry(sid.to_string()).or_default();
        match config_id {
            "mode" => entry.profile_name = Some(value.to_string()),
            "model" => entry.model_key = Some(value.to_string()),
            "thought_level" => entry.thought_level = Some(value.to_string()),
            _ => {}
        }
    }

    /// 读取但不移除指定 session 的待应用配置（用于构建 config options 展示）
    pub fn get_pending_config(&self, sid: &str) -> Option<PendingConfig> {
        self.pending_configs
            .lock()
            .expect("pending_configs poisoned")
            .get(sid)
            .cloned()
            .filter(|c| {
                c.profile_name.is_some() || c.model_key.is_some() || c.thought_level.is_some()
            })
    }

    /// 注册正在运行的 runner（存 Weak，强引用由 spawned task 持有）
    pub async fn register_running(&self, sid: &str, runner: Arc<Box<dyn AgentRunner>>) {
        self.running_runners
            .write()
            .await
            .insert(sid.to_string(), Arc::downgrade(&runner));
    }

    /// 获取正在运行的 runner（Weak::upgrade，prompt 结束后返回 None）
    pub async fn get_runner(&self, sid: &str) -> Option<Arc<Box<dyn AgentRunner>>> {
        let guard = self.running_runners.read().await;
        guard.get(sid).and_then(|w| w.upgrade())
    }
}

// ── Context trait ──────────────────────────────────────────────────

/// Handler 的统一上下文。封装 `AcpState` + `ConnectionTo<Client>`，提供常用操作。
#[async_trait]
pub(crate) trait Context: Send + Sync {
    fn builder(&self) -> &dyn AgentRunnerBuilder;
    fn put_pending_config(&self, sid: &str, key: &str, value: &str);
    fn get_pending_config(&self, sid: &str) -> Option<PendingConfig>;
    async fn register_running_runner(&self, sid: &str, runner: Arc<Box<dyn AgentRunner>>);
    async fn get_runner(&self, sid: &str) -> Option<Arc<Box<dyn AgentRunner>>>;
    fn notify_update(
        &self,
        sid: &str,
        update: SessionUpdate,
    ) -> Result<(), agent_client_protocol::Error>;
    fn notify_info(&self, sid: &str, title: &str) -> Result<(), agent_client_protocol::Error>;
    #[allow(dead_code)]
    fn notify_error(&self, sid: &str, errmsg: &str) -> Result<(), agent_client_protocol::Error>;
    fn has_flag(&self, key: &str) -> bool;
    fn set_flag(&self, key: &str, value: bool);
}

pub(crate) struct AcpStateContext {
    state: Arc<AcpState>,
    cx: ConnectionTo<Client>,
}

impl AcpStateContext {
    pub fn new(state: &Arc<AcpState>, cx: &ConnectionTo<Client>) -> Self {
        Self {
            state: state.clone(),
            cx: cx.clone(),
        }
    }
}

#[async_trait]
impl Context for AcpStateContext {
    fn builder(&self) -> &dyn AgentRunnerBuilder {
        &*self.state.builder
    }

    fn put_pending_config(&self, sid: &str, key: &str, value: &str) {
        self.state.set_pending_config(sid, key, value);
    }

    fn get_pending_config(&self, sid: &str) -> Option<PendingConfig> {
        self.state.get_pending_config(sid)
    }

    async fn register_running_runner(&self, sid: &str, runner: Arc<Box<dyn AgentRunner>>) {
        self.state.register_running(sid, runner).await;
    }

    async fn get_runner(&self, sid: &str) -> Option<Arc<Box<dyn AgentRunner>>> {
        self.state.get_runner(sid).await
    }

    fn notify_update(
        &self,
        sid: &str,
        update: SessionUpdate,
    ) -> Result<(), agent_client_protocol::Error> {
        send_notification(&self.cx, sid, update)
    }

    fn notify_info(&self, sid: &str, title: &str) -> Result<(), agent_client_protocol::Error> {
        send_fake_tool_call(&self.cx, sid, title, ToolCallStatus::Completed)
    }

    #[allow(dead_code)]
    fn notify_error(&self, sid: &str, errmsg: &str) -> Result<(), agent_client_protocol::Error> {
        send_fake_tool_call(&self.cx, sid, errmsg, ToolCallStatus::Failed)
    }

    fn has_flag(&self, key: &str) -> bool {
        match key {
            "terminal_output" => self
                .state
                .terminal_output_enabled
                .load(std::sync::atomic::Ordering::Relaxed),
            _ => false,
        }
    }

    fn set_flag(&self, key: &str, value: bool) {
        if key == "terminal_output" {
            self.state
                .terminal_output_enabled
                .store(value, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

fn send_fake_tool_call(
    cx: &ConnectionTo<Client>,
    sid: &str,
    title: impl Into<String>,
    status: ToolCallStatus,
) -> Result<(), agent_client_protocol::Error> {
    let tc = ToolCall::new(String::new(), title.into())
        .kind(agent_client_protocol::schema::v1::ToolKind::Other)
        .status(status);
    send_notification(cx, sid, SessionUpdate::ToolCall(tc))
}

#[cfg(test)]
#[path = "acp_state_test.rs"]
mod tests;
