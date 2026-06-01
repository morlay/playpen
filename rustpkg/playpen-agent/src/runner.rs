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
}

impl SimpleRunner {
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
        }
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
        Box::new(SimpleRunner {
            id: self.id.clone(),
            session: self.session.clone(),
            profile: p.into(),
            settings: self.settings.clone(),
            session_service: self.session_service.clone(),
            cancellation_token: tokio_util::sync::CancellationToken::new(),
        })
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

        // 构建工具列表
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

        let tools = crate::tool::into_tools(&toolkit);

        // 构建 LLM 客户端
        let llm_config =
            match crate::client::LlmConfig::from_settings(&self.settings, &*self.profile) {
                Ok(c) => c,
                Err(e) => {
                    let (tx, rx) = mpsc::unbounded_channel();
                    let _ = tx.send(Event::TurnStop {
                        id: String::new(),
                        stop_reason: StopReason::Error(e.to_string()),
                        token_usage: None,
                    });
                    return Box::pin(ReceiverStream { rx });
                }
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
            Err(e) => {
                let (tx, rx) = mpsc::unbounded_channel();
                let _ = tx.send(Event::TurnStop {
                    id: String::new(),
                    stop_reason: StopReason::Error(e.to_string()),
                    token_usage: None,
                });
                Box::pin(ReceiverStream { rx })
            }
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
            let _ = tx.send(Event::TurnStop {
                id: String::new(),
                stop_reason: StopReason::Error(e.to_string()),
                token_usage: None,
            });
            return Box::pin(ReceiverStream { rx });
        }

        let preamble = if instruction.is_empty() {
            None
        } else {
            Some(instruction)
        };
        let temperature = self.profile.model_profile().temperature;
        let max_turns: usize = 100;

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
    fn send(event: Event, tx: &mpsc::UnboundedSender<Event>) {
        let _ = tx.send(event);
    }

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
                send(
                    Event::TurnStop {
                        id: String::new(),
                        stop_reason: StopReason::Error(format!("{kind} persist failed: {e}")),
                        token_usage: None,
                    },
                    tx,
                );
                false
            }
        }
    }

    // 获取 session，用于后续所有 events().append_event 调用
    let session = match params.svc.get(&params.sid).await {
        Ok(s) => s,
        Err(e) => {
            send(
                Event::TurnStop {
                    id: String::new(),
                    stop_reason: StopReason::Error(e.to_string()),
                    token_usage: None,
                },
                &params.tx,
            );
            return;
        }
    };

    for _turn in 0..params.max_turns {
        if params.cancel.is_cancelled() {
            send(
                Event::TurnStop {
                    id: String::new(),
                    stop_reason: StopReason::Cancelled,
                    token_usage: None,
                },
                &params.tx,
            );
            return;
        }

        // 从 session 按事件 asc 拼装 Message
        let messages: Vec<Message> = match params.svc.get(&params.sid).await {
            Ok(s) => {
                s.events()
                    .by_role(&[
                        playpen_session::Role::User,
                        playpen_session::Role::Model,
                        playpen_session::Role::Function,
                    ])
                    .all()
                    .await
                    .pipe(events_to_chat_history)
                    .collect()
                    .await
            }
            Err(e) => {
                send(
                    Event::TurnStop {
                        id: String::new(),
                        stop_reason: StopReason::Error(e.to_string()),
                        token_usage: None,
                    },
                    &params.tx,
                );
                return;
            }
        };

        if messages.is_empty() {
            send(
                Event::TurnStop {
                    id: String::new(),
                    stop_reason: StopReason::Error("no messages to send".into()),
                    token_usage: None,
                },
                &params.tx,
            );
            return;
        }

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
        };

        match params.model.stream(request).await {
            Ok(stream) => {
                // (id, call_id, name, args) — id 来自 FunctionCall 的 event_id
                let mut pending_calls: Vec<(String, String, String, serde_json::Value)> =
                    Vec::new();

                // stream in, stream out — 惰性迭代
                let mut event_stream = Box::pin(crate::convert::process_stream(
                    stream,
                    params.extract_finish_reason,
                ));

                // 消费 event stream
                loop {
                    // 检查 cancel：取消则 drop stream 终止 LLM 请求
                    if params.cancel.is_cancelled() {
                        drop(event_stream);
                        send(
                            Event::TurnStop {
                                id: String::new(),
                                stop_reason: StopReason::Cancelled,
                                token_usage: None,
                            },
                            &params.tx,
                        );
                        return;
                    }

                    let event = match event_stream.as_mut().next().await {
                        Some(event) => event,
                        None => break,
                    };

                    match &event {
                        // delta 事件：仅发射（UI），不持久化
                        Event::ModelMessageDelta { .. } | Event::ModelThoughtDelta { .. } => {
                            send(event.clone(), &params.tx);
                        }
                        // TurnStop：有 tool_call 时跳过持久化
                        Event::TurnStop { .. } if !pending_calls.is_empty() => {
                            send(event.clone(), &params.tx);
                        }
                        // 其余最终事件（含 FunctionCall / TurnStop）：发射 + 持久化
                        _ => {
                            if !emit(event.clone(), &params.tx, session.events()).await {
                                return;
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
                        pending_calls.push((id, call_id, name, args));
                    }
                }

                // pending_calls 非空时继续执行工具。
                // 空的 TurnStop 已被跳过持久化，UI 侧仍会收到通知。
                if pending_calls.is_empty() {
                    return;
                }

                // 有 tool_call → 执行 tool，持久化 FunctionResult
                for (id, call_id, name, args) in &pending_calls {
                    match params.tools.iter().find(|t| t.name() == name) {
                        Some(tool) => {
                            let ctx = crate::tool::ToolContext::new(
                                id.clone(),
                                call_id.clone(),
                                name.clone(),
                                params.tx.clone(),
                                params.cancel.clone(),
                            );

                            match tool.execute(ctx, args.clone()).await {
                                Ok(blocks) => {
                                    if !emit(
                                        Event::FunctionResult {
                                            id: id.clone(),
                                            call_id: call_id.clone(),
                                            name: name.clone(),
                                            content: Some(blocks),
                                            code: None,
                                        },
                                        &params.tx,
                                        session.events(),
                                    )
                                    .await
                                    {
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if !emit(
                                        Event::FunctionResult {
                                            id: id.clone(),
                                            call_id: call_id.clone(),
                                            name: name.clone(),
                                            content: Some(vec![ContentBlock::text(format!(
                                                "tool error: {e}"
                                            ))]),
                                            code: Some(-1),
                                        },
                                        &params.tx,
                                        session.events(),
                                    )
                                    .await
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                        None => {
                            if !emit(
                                Event::FunctionResult {
                                    id: id.clone(),
                                    call_id: call_id.clone(),
                                    name: name.clone(),
                                    content: Some(vec![ContentBlock::text(format!(
                                        "无效的工具: {name}"
                                    ))]),
                                    code: Some(-1),
                                },
                                &params.tx,
                                session.events(),
                            )
                            .await
                            {
                                tracing::warn!(name, "emit FunctionResult 失败，跳过");
                            }
                        }
                    }
                }
                // 继续下一轮循环
            }
            Err(e) => {
                send(
                    Event::TurnStop {
                        id: String::new(),
                        stop_reason: StopReason::Error(e.to_string()),
                        token_usage: None,
                    },
                    &params.tx,
                );
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
