use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use playpen_config::Settings;
use playpen_content::{ContentBlock, Event, StopReason};
use playpen_profile::{AgentProfile, AgentProfileLoader};
use playpen_session::{Session, SessionService};
use rig_core::OneOrMany;
use rig_core::completion::{CompletionModel, CompletionRequest, Message, ToolDefinition};
use serde_json;
use tokio::sync::mpsc;

use crate::client::LlmClient;
use crate::convert::{StreamPipe, events_to_chat_history};
use crate::subagent::SubagentHost;
use crate::tool::Tool;

pub const PROFILE_STATE_KEY_NAME: &str = "agent-profile:name";
pub const PROFILE_STATE_KEY_MODEL_PROFILE: &str = "agent-profile:model-profile";
pub const PROFILE_STATE_KEY_INSTRUCTION: &str = "agent-profile:instruction";
pub const PROFILE_STATE_KEY_TOOL_SCHEMA: &str = "agent-profile:tool-schema";

// ── Traits ──────────────────────────────────────────────────────────

#[async_trait]
pub trait AgentRunnerBuilder: Send + Sync {
    async fn create(&self, p: Box<dyn AgentProfile>) -> anyhow::Result<Box<dyn AgentRunner>>;

    async fn resume(&self, id: &str) -> anyhow::Result<Box<dyn AgentRunner>>;

    fn agent_profiles(&self) -> anyhow::Result<Vec<Box<dyn AgentProfile>>>;

    fn sessions(&self) -> &dyn SessionService;
}

#[async_trait]
pub trait AgentRunner: Send + Sync {
    fn id(&self) -> &str;
    fn session(&self) -> &dyn Session;
    fn profile(&self) -> &dyn AgentProfile;
    fn settings(&self) -> &Settings;
    fn with_profile(&self, p: Box<dyn AgentProfile>) -> Box<dyn AgentRunner>;
    /// 返回注入了子代理宿主的新 runner（profile 继承自 self）。
    /// 与 `with_profile` 同构：重建 runner、不修改 self。注入后 `run()` 会附加 spawn_agent 工具。
    fn with_subagent_host(&self, builder: Arc<dyn AgentRunnerBuilder>) -> Box<dyn AgentRunner>;

    async fn run(&self, prompt: Vec<ContentBlock>) -> Pin<Box<dyn Stream<Item = Event> + Send>>;

    async fn rewind(&self) -> anyhow::Result<()>;
    fn replay(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>>;
    async fn cancel(&self);
}

// ── SimpleRunnerBuilder ─────────────────────────────────────────────

pub struct SimpleRunnerBuilder {
    working_dir: std::path::PathBuf,
    settings: Settings,
    session_service: Arc<dyn SessionService>,
    profile_resolver: Arc<dyn AgentProfileLoader>,
}

impl SimpleRunnerBuilder {
    pub fn new(
        settings: &Settings,
        dirs: &playpen_config::Dirs,
        session_service: Arc<dyn SessionService>,
        profile_resolver: Arc<dyn AgentProfileLoader>,
    ) -> Self {
        Self {
            working_dir: dirs.working_dir.clone(),
            settings: settings.clone(),
            session_service,
            profile_resolver,
        }
    }
}

#[async_trait]
impl AgentRunnerBuilder for SimpleRunnerBuilder {
    async fn create(&self, p: Box<dyn AgentProfile>) -> anyhow::Result<Box<dyn AgentRunner>> {
        let session = self.session_service.create().await?;
        let sid = session.id().to_string();

        Ok(Box::new(SimpleRunner::new(
            sid,
            session,
            p,
            self.settings.clone(),
            self.session_service.clone(),
        )))
    }

    async fn resume(&self, id: &str) -> anyhow::Result<Box<dyn AgentRunner>> {
        let session = self
            .session_service
            .get(id)
            .await
            .map_err(|_| anyhow::anyhow!("session {id} 不存在"))?;

        // 历史数据补偿：孤儿 FunctionCall（无配对 FunctionResult）补发已取消的结果。
        // 否则历史中残留无 ToolResult 配对的 ToolCall，下一次请求会被 LLM API 拒绝。
        if let Err(e) = reconcile_orphan_function_calls(&*session).await {
            tracing::warn!(session_id = id, error = %e, "reconcile orphan function calls failed");
        }

        let sp = SessionProfile {
            name: session
                .state()
                .get(PROFILE_STATE_KEY_NAME)
                .await
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "default".into()),
            model_profile: session
                .state()
                .get(PROFILE_STATE_KEY_MODEL_PROFILE)
                .await
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default(),
            instruction: session
                .state()
                .get(PROFILE_STATE_KEY_INSTRUCTION)
                .await
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default(),
        };

        let profiles = self
            .profile_resolver
            .agent_profiles(&playpen_config::Dirs::with_defaults(&self.working_dir))?;

        let profile = profiles
            .into_iter()
            .find(|p| p.name() == sp.name)
            .or_else(|| {
                self.profile_resolver
                    .agent_profiles(&playpen_config::Dirs::with_defaults(&self.working_dir))
                    .ok()?
                    .into_iter()
                    .next()
            })
            .map(|p| p.with_model_profile(&|_| sp.model_profile.clone()))
            .ok_or_else(|| anyhow::anyhow!("profile '{}' not found", sp.name))?;

        Ok(Box::new(SimpleRunner::new(
            id.to_string(),
            session,
            profile,
            self.settings.clone(),
            self.session_service.clone(),
        )))
    }

    fn agent_profiles(&self) -> anyhow::Result<Vec<Box<dyn AgentProfile>>> {
        self.profile_resolver
            .agent_profiles(&playpen_config::Dirs::with_defaults(&self.working_dir))
    }

    fn sessions(&self) -> &dyn SessionService {
        &*self.session_service
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SessionProfile {
    pub name: String,
    pub model_profile: playpen_config::model::ModelProfile,
    pub instruction: String,
}

// ── SimpleRunner ────────────────────────────────────────────────────

pub struct SimpleRunner {
    id: String,
    session: Arc<dyn Session>,
    profile: Arc<dyn AgentProfile>,
    settings: Settings,
    session_service: Arc<dyn SessionService>,
    cancellation_token: tokio_util::sync::CancellationToken,
    /// 子代理宿主：`Some` 时 `run()` 会附加 spawn_agent 工具，支持嵌套子代理。
    subagent_host: Option<Arc<dyn SubagentHost>>,
}

impl SimpleRunner {
    /// `with_profile` 的具体类型版本：返回新的 `SimpleRunner`（保留 subagent_host 与取消令牌），
    /// 供包装类型（如测试的 FakeRunner）复用具体类型。
    pub fn with_profile_typed(&self, p: Box<dyn AgentProfile>) -> SimpleRunner {
        SimpleRunner {
            id: self.id.clone(),
            session: self.session.clone(),
            profile: p.into(),
            settings: self.settings.clone(),
            session_service: self.session_service.clone(),
            cancellation_token: self.cancellation_token.clone(),
            subagent_host: self.subagent_host.clone(),
        }
    }

    /// `with_subagent_host` 的具体类型版本：注入子代理宿主，返回新的 `SimpleRunner`。
    ///
    /// 取消令牌与 self **共享**（而非新建）：cancel 任一视图即取消同一逻辑 runner；
    /// 同时把令牌传给宿主，父会话 cancel 时级联取消子代理。
    pub fn with_subagent_host_typed(&self, builder: Arc<dyn AgentRunnerBuilder>) -> SimpleRunner {
        SimpleRunner {
            id: self.id.clone(),
            session: self.session.clone(),
            profile: self.profile.clone(),
            settings: self.settings.clone(),
            session_service: self.session_service.clone(),
            cancellation_token: self.cancellation_token.clone(),
            subagent_host: Some(Arc::new(crate::subagent::RunnerSubagentHost::new(
                builder,
                self.profile.clone(),
                self.cancellation_token.clone(),
            ))),
        }
    }

    pub fn new(
        id: String,
        session: Box<dyn Session>,
        profile: Box<dyn AgentProfile>,
        settings: Settings,
        session_service: Arc<dyn SessionService>,
    ) -> Self {
        Self {
            id,
            session: Arc::from(session),
            profile: Arc::from(profile),
            settings,
            session_service,
            cancellation_token: tokio_util::sync::CancellationToken::new(),
            subagent_host: None,
        }
    }

    /// 构建运行工具列表：Toolkit 默认工具（read/edit/write/grep/find/move/webfetch/bash，
    /// 按 profile.working_dir 与 sandbox 配置初始化）+ 注入子代理宿主时附加 spawn_agent。
    ///
    /// 主 runner 与子代理 runner（均为 SimpleRunner）走同一方法——子代理的工具注册与主
    /// agent 完全一致。最终按 `profile.tool_enabled` 过滤发生在 `build_tools_and_defs`。
    pub(crate) fn build_run_tools(&self) -> Vec<Arc<dyn crate::tool::Tool>> {
        let mut toolkit = playpen_toolkit::Toolkit::defaults(self.profile.working_dir());
        if let Some(ref profile) = self.settings.sandbox
            && profile.enabled
        {
            // SandboxProfile 与 Config 同源自 [sandbox] TOML，通过 serde 转换
            if let Ok(config) = serde_json::from_value::<playpen_sandbox::config::Config>(
                serde_json::to_value(profile).expect("SandboxProfile 序列化不应失败"),
            ) {
                let sandbox = playpen_sandbox::create(&config, self.profile.working_dir());
                toolkit = toolkit.with_sandbox(sandbox);
            }
        }

        let mut tools = crate::tool::into_tools(&toolkit);
        if let Some(host) = &self.subagent_host {
            tools.push(Arc::new(crate::tool::SpawnAgentTool::new(host.clone())));
        }
        tools
    }
}

#[async_trait]
impl AgentRunner for SimpleRunner {
    fn id(&self) -> &str {
        &self.id
    }

    fn session(&self) -> &dyn Session {
        &*self.session
    }

    fn profile(&self) -> &dyn AgentProfile {
        &*self.profile
    }

    fn settings(&self) -> &Settings {
        &self.settings
    }

    fn with_profile(&self, p: Box<dyn AgentProfile>) -> Box<dyn AgentRunner> {
        Box::new(self.with_profile_typed(p))
    }

    fn with_subagent_host(&self, builder: Arc<dyn AgentRunnerBuilder>) -> Box<dyn AgentRunner> {
        Box::new(self.with_subagent_host_typed(builder))
    }

    async fn run(&self, prompt: Vec<ContentBlock>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        // 持久化 name / model_profile
        {
            let profile_name = self.profile.name().to_string();
            let profile_model =
                serde_json::to_value(self.profile.model_profile()).unwrap_or_default();
            if let Err(e) = self
                .session
                .events()
                .append(&Event::StateUpdate {
                    id: String::new(),
                    name: PROFILE_STATE_KEY_NAME.into(),
                    data: serde_json::json!(profile_name),
                })
                .await
            {
                tracing::warn!(error = %e, "persist profile name failed");
            }
            if let Err(e) = self
                .session
                .events()
                .append(&Event::StateUpdate {
                    id: String::new(),
                    name: PROFILE_STATE_KEY_MODEL_PROFILE.into(),
                    data: profile_model,
                })
                .await
            {
                tracing::warn!(error = %e, "persist profile model failed");
            }
        }

        // 构建工具列表（Toolkit 默认工具 + 有宿主时附加 spawn_agent）
        let tools = self.build_run_tools();

        // 构建 LLM 客户端
        let llm_config =
            match crate::client::LlmConfig::from_settings(&self.settings, &*self.profile) {
                Ok(c) => c,
                Err(e) => return stop_stream(StopReason::Error(e.to_string())),
            };

        let model_max_tokens = llm_config.model_config.as_ref().map(|m| m.max_tokens);

        let client = LlmClient::new(llm_config);

        let additional_params =
            client.build_additional_params(self.profile.model_profile(), model_max_tokens);

        match client.build_model() {
            Ok(crate::client::ModelEnum::Deepseek {
                model,
                extract_finish_reason,
            }) => {
                self.run_with_model(
                    model,
                    prompt,
                    tools,
                    additional_params,
                    extract_finish_reason,
                )
                .await
            }
            Ok(crate::client::ModelEnum::Openai {
                model,
                extract_finish_reason,
            }) => {
                self.run_with_model(
                    model,
                    prompt,
                    tools,
                    additional_params,
                    extract_finish_reason,
                )
                .await
            }
            Err(e) => stop_stream(StopReason::Error(e.to_string())),
        }
    }

    fn replay(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let sid = self.id.clone();
        let svc = self.session_service.clone();

        tokio::spawn(async move {
            if let Ok(session) = svc.get(&sid).await {
                let events: Vec<Event> = session.events().all().await.collect().await;
                for ev in events {
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
            }
        });

        Box::pin(ReceiverStream { rx })
    }

    async fn rewind(&self) -> anyhow::Result<()> {
        use futures::StreamExt;
        let session = self.session_service.get(&self.id).await?;
        let user_msgs: Vec<Event> = session
            .events()
            .by_role(&[playpen_session::Role::User])
            .all()
            .await
            .collect()
            .await;
        if let Some(Event::UserMessage { id: eid, .. }) = user_msgs.last() {
            self.session_service.rewind(eid).await?;
        }
        Ok(())
    }

    async fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}

impl SimpleRunner {
    /// 带 tool 循环。接受已构建的 model，便于测试注入 MockCompletionModel。
    ///
    /// 每次循环从 session 按事件 asc 拼装 Message，stream 转换逻辑委派给
    /// [`crate::convert::process_stream`]，runner 只关心「有没有 tool_call → 执行并继续」。
    pub async fn run_with_model<M: CompletionModel + 'static>(
        &self,
        model: M,
        prompt: Vec<ContentBlock>,
        tools: Vec<std::sync::Arc<dyn Tool>>,
        additional_params: Option<serde_json::Value>,
        extract_finish_reason: fn(&dyn std::any::Any) -> Option<String>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let instruction = self.instruction().await;

        // 状态持久化 + 工具构建
        let (tools, tool_defs) = self.build_tools_and_defs(tools);
        self.persist_run_state(&instruction, &tool_defs).await;

        // 持久化 user message
        if let Err(e) = self.append_user_message(prompt).await {
            send_stop(&tx, StopReason::Error(e.to_string()));
            return Box::pin(ReceiverStream { rx });
        }

        let preamble = if instruction.is_empty() {
            None
        } else {
            Some(instruction)
        };
        let temperature = self.profile.model_profile().temperature;
        let max_turns: usize = 200;

        let sid = self.id.clone();
        let svc = self.session_service.clone();
        let cancel = self.cancellation_token.clone();

        tokio::spawn(run_tool_loop(ToolLoopParams {
            svc,
            sid,
            model,
            tx,
            cancel,
            tools,
            tool_defs,
            preamble,
            temperature,
            additional_params,
            max_turns,
            extract_finish_reason,
        }));

        Box::pin(ReceiverStream { rx })
    }

    /// 按 profile 过滤工具 + 生成 ToolDefinition。
    fn build_tools_and_defs(
        &self,
        tools: Vec<Arc<dyn Tool>>,
    ) -> (Vec<Arc<dyn Tool>>, Vec<ToolDefinition>) {
        let tools: Vec<Arc<dyn Tool>> = tools
            .into_iter()
            .filter(|t| self.profile.tool_enabled(t.name()))
            .collect();
        let tool_defs = crate::tool::to_tool_definitions(&tools);
        (tools, tool_defs)
    }

    /// 持久化 instruction 和 tool_schema（仅首次不存在时写入）。
    async fn persist_run_state(&self, instruction: &str, tool_defs: &[ToolDefinition]) {
        // instruction 尚未持久化时写入 state
        if self
            .session
            .state()
            .get(PROFILE_STATE_KEY_INSTRUCTION)
            .await
            .is_none()
            && let Err(e) = self.set_instruction(instruction).await
        {
            tracing::warn!(error = %e, "persist instruction failed");
        }

        // tool_defs 尚未持久化时写入 state
        if self
            .session
            .state()
            .get(PROFILE_STATE_KEY_TOOL_SCHEMA)
            .await
            .is_none()
            && let Err(e) = self
                .session
                .events()
                .append(&Event::StateUpdate {
                    id: String::new(),
                    name: PROFILE_STATE_KEY_TOOL_SCHEMA.into(),
                    data: serde_json::to_value(tool_defs).unwrap_or_default(),
                })
                .await
        {
            tracing::warn!(error = %e, "persist tool_schema failed");
        }
    }

    /// 持久化 user message。
    async fn append_user_message(&self, prompt: Vec<ContentBlock>) -> anyhow::Result<()> {
        self.session
            .events()
            .append(&Event::UserMessage {
                id: String::new(),
                content: prompt,
            })
            .await?;
        Ok(())
    }

    pub(crate) async fn instruction(&self) -> String {
        match self.session_service.get(&self.id).await {
            Ok(session) => match session.state().get(PROFILE_STATE_KEY_INSTRUCTION).await {
                Some(v) => v.as_str().map(|s| s.to_string()).unwrap_or_default(),
                None => self.profile.instructions().unwrap_or_default(),
            },
            Err(_) => self.profile.instructions().unwrap_or_default(),
        }
    }

    pub(crate) async fn set_instruction(&self, instruction: &str) -> anyhow::Result<()> {
        self.session
            .events()
            .append(&Event::StateUpdate {
                id: String::new(),
                name: PROFILE_STATE_KEY_INSTRUCTION.into(),
                data: serde_json::json!(instruction),
            })
            .await?;
        Ok(())
    }
}

const MODEL_ACTION_THOUGHT: u8 = 1 << 0;
const MODEL_ACTION_MESSAGE: u8 = 1 << 1;
const MODEL_ACTION_CALL: u8 = 1 << 2;

// ── Stream wrapper ──────────────────────────────────────────────────

pub(crate) struct ReceiverStream {
    rx: mpsc::UnboundedReceiver<Event>,
}

impl Stream for ReceiverStream {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

/// 构造一个仅发射单个终止事件（TurnStop）的流，用于提前中止场景。
fn stop_stream(reason: StopReason) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
    let (tx, rx) = mpsc::unbounded_channel();
    let _ = tx.send(Event::TurnStop {
        id: String::new(),
        stop_reason: reason,
        token_usage: None,
    });
    Box::pin(ReceiverStream { rx })
}

// ── Tool loop ───────────────────────────────────────────────────────

/// `run_tool_loop` 的参数聚合。
struct ToolLoopParams<M: CompletionModel + 'static> {
    svc: Arc<dyn SessionService>,
    sid: String,
    model: M,
    tx: mpsc::UnboundedSender<Event>,
    cancel: tokio_util::sync::CancellationToken,
    tools: Vec<Arc<dyn Tool>>,
    tool_defs: Vec<ToolDefinition>,
    preamble: Option<String>,
    temperature: Option<f64>,
    additional_params: Option<serde_json::Value>,
    max_turns: usize,
    extract_finish_reason: fn(&dyn std::any::Any) -> Option<String>,
}

/// 工具循环：从 session 读取事件 → 请求 LLM → 消费 stream → 执行 tool call → 重复。
async fn run_tool_loop<M: CompletionModel + 'static>(params: ToolLoopParams<M>) {
    // 获取 session，用于后续所有 events().append 调用
    let session = match params.svc.get(&params.sid).await {
        Ok(s) => s,
        Err(e) => {
            send_stop(&params.tx, StopReason::Error(e.to_string()));
            return;
        }
    };

    for _turn in 0..params.max_turns {
        if params.cancel.is_cancelled() {
            send_stop(&params.tx, StopReason::Cancelled);
            return;
        }

        // 从 session 按事件 asc 拼装 Message
        let messages = match load_chat_messages(&*params.svc, &params.sid).await {
            Ok(m) => m,
            Err(e) => {
                send_stop(&params.tx, StopReason::Error(e));
                return;
            }
        };

        let request = CompletionRequest {
            model: None,
            preamble: params.preamble.clone(),
            chat_history: OneOrMany::many(messages).expect("messages non-empty"),
            documents: vec![],
            tools: params.tool_defs.clone(),
            temperature: params.temperature,
            max_tokens: None,
            tool_choice: None,
            additional_params: params.additional_params.clone(),
            output_schema: None,
            record_telemetry_content: false,
        };

        match params.model.stream(request).await {
            Ok(stream) => {
                // stream in, stream out — 惰性迭代
                let event_stream = Box::pin(crate::convert::process_stream(
                    stream,
                    params.extract_finish_reason,
                ));

                let (pending_calls, turn_bits) =
                    match consume_turn_stream(
                        event_stream,
                        &params.tx,
                        session.events(),
                        &params.cancel,
                    )
                    .await
                    {
                        ConsumeOutcome::Done { pending_calls, turn_bits } => {
                            (pending_calls, turn_bits)
                        }
                        ConsumeOutcome::Cancelled { pending_calls } => {
                            // 已持久化的 FunctionCall 必须补发 FunctionResult，
                            // 否则 session 历史残留孤儿 tool_call（无配对结果），
                            // 下一次构建请求会被 LLM API 拒绝。
                            if !emit_cancelled_results(&pending_calls, &params.tx, session.events())
                                .await
                            {
                                // emit_cancelled_results 已发 TurnStop::Error
                                return;
                            }
                            send_stop(&params.tx, StopReason::Cancelled);
                            return;
                        }
                        ConsumeOutcome::PersistFailed => return,
                    };

                // 位判断：只有 thought 时重试（LLM 抽风防护）
                if turn_bits & (MODEL_ACTION_MESSAGE | MODEL_ACTION_CALL) == 0 {
                    if turn_bits & MODEL_ACTION_THOUGHT != 0 {
                        continue;
                    }
                    return;
                }

                if pending_calls.is_empty() {
                    return;
                }

                // 有 tool_call → 执行 tool，持久化 FunctionResult
                for call in &pending_calls {
                    if !execute_tool_call(
                        &params.tools,
                        call,
                        &params.tx,
                        session.events(),
                        &params.cancel,
                    )
                    .await
                    {
                        return;
                    }
                }
                // 继续下一轮循环
            }
            Err(e) => {
                send_stop(&params.tx, StopReason::Error(e.to_string()));
                return;
            }
        }
    }

    // max turns reached
    emit(
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
        &params.tx,
        session.events(),
    )
    .await;
}

// ── Tool loop helpers ───────────────────────────────────────────────

/// 一次模型输出中收集到的待执行工具调用。
#[derive(Debug)]
struct PendingCall {
    id: String,
    call_id: String,
    name: String,
    args: serde_json::Value,
}

/// 单轮 stream 消费的结果。
#[derive(Debug)]
enum ConsumeOutcome {
    /// 正常消费完，附带待执行调用与本轮产出的事件类型位标志。
    Done { pending_calls: Vec<PendingCall>, turn_bits: u8 },
    /// 用户取消：流已 drop。携带已持久化的待执行调用，
    /// 调用方必须为它们补发 FunctionResult，否则 session 历史残留孤儿
    /// tool_call，下一次请求会被 LLM API 拒绝（整个 session 作废）。
    Cancelled { pending_calls: Vec<PendingCall> },
    /// 持久化失败：已发送 `TurnStop::Error`，调用方应直接终止。
    PersistFailed,
}

/// 发送终止事件到 tx。
fn send_stop(tx: &mpsc::UnboundedSender<Event>, reason: StopReason) {
    let _ = tx.send(Event::TurnStop {
        id: String::new(),
        stop_reason: reason,
        token_usage: None,
    });
}

/// 发射事件到 tx 并持久化到 session；持久化失败时发送 `TurnStop::Error` 并返回 false。
async fn emit(
    event: Event,
    tx: &mpsc::UnboundedSender<Event>,
    events: &dyn playpen_session::Events,
) -> bool {
    let kind = event_kind(&event);
    let _ = tx.send(event.clone());

    match events.append(&event).await {
        Ok(_) => true,
        Err(e) => {
            tracing::error!(error = %e, kind, "persist failed, aborting loop");
            send_stop(tx, StopReason::Error(format!("{kind} persist failed: {e}")));
            false
        }
    }
}

/// 从 session 按角色过滤拼装 Message。
/// Err 携带无法继续的原因（session 获取失败 / 无消息）。
async fn load_chat_messages(
    svc: &dyn SessionService,
    sid: &str,
) -> Result<Vec<Message>, String> {
    let s = svc.get(sid).await.map_err(|e| e.to_string())?;
    let events: Vec<Event> = s
        .events()
        .by_role(&[
            playpen_session::Role::User,
            playpen_session::Role::Model,
            playpen_session::Role::Function,
        ])
        .all()
        .await
        .collect()
        .await;

    // 兜底：丢弃无对应 FunctionResult 的孤儿 FunctionCall。
    // 否则转换层会产出无 ToolResult 配对的 ToolCall，LLM API 直接拒绝请求。
    // （正常路径下 cancel 会补发 cancelled 的 FunctionResult，此处仅防御异常/历史数据）
    let resulted_call_ids: std::collections::HashSet<String> = events
        .iter()
        .filter_map(|e| match e {
            Event::FunctionResult { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();

    let messages: Vec<Message> = futures::stream::iter(events.into_iter().filter(|e| match e {
        Event::FunctionCall { call_id, .. } => resulted_call_ids.contains(call_id),
        _ => true,
    }))
    .pipe(events_to_chat_history)
    .collect()
    .await;

    if messages.is_empty() {
        return Err("no messages to send".into());
    }
    Ok(messages)
}

/// 消费单轮 LLM 输出流：delta 仅发射，最终事件发射 + 持久化，收集待执行调用。
///
/// delta 事件（`ModelMessageDelta` / `ModelThoughtDelta`）与带 tool_call 的
/// `TurnStop` 仅发射不持久化；其余最终事件发射 + 持久化。
async fn consume_turn_stream(
    mut event_stream: Pin<Box<dyn Stream<Item = Event> + Send>>,
    tx: &mpsc::UnboundedSender<Event>,
    events: &dyn playpen_session::Events,
    cancel: &tokio_util::sync::CancellationToken,
) -> ConsumeOutcome {
    let mut pending_calls: Vec<PendingCall> = Vec::new();
    let mut turn_bits: u8 = 0;

    loop {
        // 取消则 drop stream 终止 LLM 请求
        if cancel.is_cancelled() {
            drop(event_stream);
            return ConsumeOutcome::Cancelled { pending_calls };
        }

        let event = match event_stream.as_mut().next().await {
            Some(event) => event,
            None => break,
        };

        match &event {
            // delta 事件：仅发射（UI），不持久化
            Event::ModelMessageDelta { .. } | Event::ModelThoughtDelta { .. } => {
                let _ = tx.send(event.clone());
            }
            // TurnStop：有 tool_call 时跳过持久化
            Event::TurnStop { .. } if !pending_calls.is_empty() => {
                let _ = tx.send(event.clone());
            }
            // ModelThought / ModelMessage / FunctionCall：记录本轮产出类型 + 发射 + 持久化
            Event::ModelThought { .. } => {
                turn_bits |= MODEL_ACTION_THOUGHT;
                if !emit(event.clone(), tx, events).await {
                    return ConsumeOutcome::PersistFailed;
                }
            }
            Event::ModelMessage { .. } => {
                turn_bits |= MODEL_ACTION_MESSAGE;
                if !emit(event.clone(), tx, events).await {
                    return ConsumeOutcome::PersistFailed;
                }
            }
            Event::FunctionCall { .. } => {
                turn_bits |= MODEL_ACTION_CALL;
                if !emit(event.clone(), tx, events).await {
                    return ConsumeOutcome::PersistFailed;
                }
            }
            // 其余最终事件（含无 tool_call 的 TurnStop）：发射 + 持久化
            _ => {
                if !emit(event.clone(), tx, events).await {
                    return ConsumeOutcome::PersistFailed;
                }
            }
        }

        if let Event::FunctionCall {
            id,
            call_id,
            name,
            args,
            ..
        } = event
        {
            pending_calls.push(PendingCall { id, call_id, name, args });
        }
    }

    ConsumeOutcome::Done { pending_calls, turn_bits }
}

/// 构造并持久化 FunctionResult。返回 false 表示持久化失败（已发 `TurnStop::Error`）。
async fn emit_result(
    call: &PendingCall,
    content: Vec<ContentBlock>,
    code: Option<i32>,
    tx: &mpsc::UnboundedSender<Event>,
    events: &dyn playpen_session::Events,
) -> bool {
    emit(
        Event::FunctionResult {
            id: call.id.clone(),
            call_id: call.call_id.clone(),
            name: call.name.clone(),
            content: Some(content),
            code,
        },
        tx,
        events,
    )
    .await
}

/// 补偿历史数据：为 session 中无配对 FunctionResult 的孤儿 FunctionCall
/// 补发已取消的结果（与 cancel 路径的 `emit_cancelled_results` 一致）。
/// 幂等：已有配对的不重复补。返回补发的数量。
pub(crate) async fn reconcile_orphan_function_calls(
    session: &dyn Session,
) -> anyhow::Result<usize> {
    let events: Vec<Event> = session.events().all().await.collect().await;

    let resulted_call_ids: std::collections::HashSet<&str> = events
        .iter()
        .filter_map(|e| match e {
            Event::FunctionResult { call_id, .. } => Some(call_id.as_str()),
            _ => None,
        })
        .collect();

    let mut repaired = 0usize;
    for e in &events {
        let Event::FunctionCall { id, call_id, name, .. } = e else {
            continue;
        };
        if resulted_call_ids.contains(call_id.as_str()) {
            continue;
        }
        session
            .events()
            .append(&Event::FunctionResult {
                id: id.clone(),
                call_id: call_id.clone(),
                name: name.clone(),
                content: Some(vec![ContentBlock::text("工具调用已取消")]),
                code: None,
            })
            .await?;
        repaired += 1;
    }
    Ok(repaired)
}

/// 为已持久化的待执行调用补发 cancelled 的 FunctionResult。
/// 返回 false 表示持久化失败（已发 `TurnStop::Error`），调用方应终止。
async fn emit_cancelled_results(
    calls: &[PendingCall],
    tx: &mpsc::UnboundedSender<Event>,
    events: &dyn playpen_session::Events,
) -> bool {
    for call in calls {
        if !emit_result(
            call,
            vec![ContentBlock::text("工具调用已取消")],
            None,
            tx,
            events,
        )
        .await
        {
            return false;
        }
    }
    true
}

/// 执行单个待调用工具并持久化 FunctionResult。
/// 返回 false 表示持久化失败（已发 `TurnStop::Error`），调用方应终止；
/// 无效工具的结果持久化失败仅 warn（与原行为一致）。
async fn execute_tool_call(
    tools: &[Arc<dyn Tool>],
    call: &PendingCall,
    tx: &mpsc::UnboundedSender<Event>,
    events: &dyn playpen_session::Events,
    cancel: &tokio_util::sync::CancellationToken,
) -> bool {
    match tools.iter().find(|t| t.name() == call.name) {
        Some(tool) => {
            let ctx = crate::tool::ToolContext::new(
                call.id.clone(),
                call.call_id.clone(),
                call.name.clone(),
                tx.clone(),
                cancel.clone(),
            );
            match tool.execute(ctx, call.args.clone()).await {
                Ok(blocks) => emit_result(call, blocks, None, tx, events).await,
                Err(e) => {
                    emit_result(
                        call,
                        vec![ContentBlock::text(format!("tool error: {e}"))],
                        Some(-1),
                        tx,
                        events,
                    )
                    .await
                }
            }
        }
        None => {
            let ok = emit_result(
                call,
                vec![ContentBlock::text(format!("无效的工具: {}", call.name))],
                Some(-1),
                tx,
                events,
            )
            .await;
            if !ok {
                tracing::warn!(name = %call.name, "emit FunctionResult 失败，跳过");
            }
            true
        }
    }
}

// ── helpers ─────────────────────────────────────────────────────────

fn event_kind(event: &Event) -> &'static str {
    match event {
        Event::UserMessage { .. } => "user_message",
        Event::ModelMessage { .. } | Event::ModelMessageDelta { .. } => "model_message",
        Event::ModelThought { .. } | Event::ModelThoughtDelta { .. } => "model_thought",
        Event::FunctionCall { .. } => "function_call",
        Event::FunctionOutputDelta { .. } => "function_output",
        Event::FunctionResult { .. } => "function_result",
        Event::TurnStop { .. } => "turn_stop",
        Event::StateUpdate { .. } => "state",
    }
}

#[cfg(test)]
#[path = "runner_test.rs"]
mod tests;
