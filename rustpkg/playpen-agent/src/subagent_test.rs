use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use playpen_config::Settings;
use playpen_content::{ContentBlock, Event, StopReason};
use playpen_profile::AgentProfile;
use playpen_session::{DBSessionService, Session, SessionService};
use rig_core::test_utils::{MockCompletionModel, MockStreamEvent, MockTurn};

use crate::runner::{AgentRunner, AgentRunnerBuilder, SimpleRunner};
use crate::subagent::{RunnerSubagentHost, SubagentHost, entry_count};
use crate::testing::{TestProfile, make_runner};

async fn new_db() -> Arc<dyn SessionService> {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    let svc = DBSessionService::new(db);
    svc.migrate().await.unwrap();
    Arc::new(svc)
}

// ── FakeRunner：SimpleRunner + mock LLM ──────────────────────────────

struct FakeRunner {
    inner: SimpleRunner,
    llm: MockCompletionModel,
    delay: std::time::Duration,
}

impl FakeRunner {
    fn new(inner: SimpleRunner, llm: MockCompletionModel, delay: std::time::Duration) -> Self {
        Self { inner, llm, delay }
    }
}

#[async_trait]
impl AgentRunner for FakeRunner {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn session(&self) -> &dyn Session {
        self.inner.session()
    }
    fn profile(&self) -> &dyn AgentProfile {
        self.inner.profile()
    }
    fn settings(&self) -> &Settings {
        self.inner.settings()
    }
    fn with_profile(&self, p: Box<dyn AgentProfile>) -> Box<dyn AgentRunner> {
        Box::new(FakeRunner {
            inner: self.inner.with_profile_typed(p),
            llm: self.llm.clone(),
            delay: self.delay,
        })
    }
    fn with_subagent_host(&self, b: Arc<dyn AgentRunnerBuilder>) -> Box<dyn AgentRunner> {
        Box::new(FakeRunner {
            inner: self.inner.with_subagent_host_typed(b),
            llm: self.llm.clone(),
            delay: self.delay,
        })
    }
    async fn run(&self, prompt: Vec<ContentBlock>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        self.inner
            .run_with_model(self.llm.clone(), prompt, vec![], None, None)
            .await
    }
    async fn rewind(&self) -> anyhow::Result<()> {
        self.inner.rewind().await
    }
    fn replay(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        self.inner.replay()
    }
    async fn cancel(&self) {
        self.inner.cancel().await
    }
}

// ── FakeBuilder ─────────────────────────────────────────────────────

struct FakeBuilder {
    svc: Arc<dyn SessionService>,
    llm: MockCompletionModel,
    /// 子代理 run 前的模拟延迟（测试 cancel 级联用）
    delay: std::time::Duration,
    /// create 收到的 profile 摘要 (name, working_dir, model)
    created: Arc<std::sync::Mutex<Vec<(String, String, String)>>>,
}

#[async_trait]
impl AgentRunnerBuilder for FakeBuilder {
    async fn create(&self, p: Box<dyn AgentProfile>) -> anyhow::Result<Box<dyn AgentRunner>> {
        self.created.lock().unwrap().push((
            p.name().to_string(),
            p.working_dir().display().to_string(),
            p.model_profile().model.clone(),
        ));
        let session = self.svc.create().await?;
        let sid = session.id().to_string();
        Ok(Box::new(FakeRunner::new(
            SimpleRunner::new(sid, session, p, Settings::default(), self.svc.clone()),
            self.llm.clone(),
            self.delay,
        )))
    }
    async fn resume(&self, id: &str) -> anyhow::Result<Box<dyn AgentRunner>> {
        let session = self.svc.get(id).await?;
        Ok(Box::new(FakeRunner::new(
            SimpleRunner::new(
                id.to_string(),
                session,
                Box::new(TestProfile::default()),
                Settings::default(),
                self.svc.clone(),
            ),
            self.llm.clone(),
            self.delay,
        )))
    }
    fn agent_profiles(&self) -> anyhow::Result<Vec<Box<dyn AgentProfile>>> {
        Ok(vec![Box::new(TestProfile::default())])
    }
    fn sessions(&self) -> &dyn SessionService {
        &*self.svc
    }
}

fn make_host(svc: Arc<dyn SessionService>, llm: MockCompletionModel) -> RunnerSubagentHost {
    make_host_with_cancel(svc, llm, tokio_util::sync::CancellationToken::new())
}

fn make_host_with_cancel(
    svc: Arc<dyn SessionService>,
    llm: MockCompletionModel,
    parent_cancel: tokio_util::sync::CancellationToken,
) -> RunnerSubagentHost {
    let builder: Arc<dyn AgentRunnerBuilder> = Arc::new(FakeBuilder {
        svc,
        llm,
        delay: std::time::Duration::ZERO,
        created: Arc::new(std::sync::Mutex::new(Vec::new())),
    });
    RunnerSubagentHost::new(builder, Arc::new(TestProfile::default()), parent_cancel)
}

// ── tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_entry_count_counts_visible_events_only() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();

    // 可见（4 类）：UserMessage / ModelMessage / ModelThought / FunctionCall
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("u")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: "k".into(),
            data: serde_json::json!(1),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("m")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "read".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionResult {
            id: String::new(),
            call_id: "c1".into(),
            name: "read".into(),
            content: None,
            code: None,
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelThought {
            id: String::new(),
            text: "t".into(),
        })
        .await
        .unwrap();

    assert_eq!(entry_count(&*session).await, 4, "仅计四类可见事件");
}

#[tokio::test]
async fn test_spawn_create_and_send() {
    let svc = new_db().await;
    let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("子任务结果")]]);
    let host = make_host(svc, llm);

    let handle = host.spawn("计算", None).await.unwrap();
    assert!(!handle.session_id.is_empty(), "新会话应有 session_id");
    assert_eq!(handle.message_start_index, 0, "首轮 start 应为 0");

    let output = host.send(&handle, "请计算").await.unwrap();
    assert_eq!(output.text, "子任务结果");
    assert!(
        output.message_end_index >= 1,
        "end 索引应指向最后一条可见事件"
    );
}

#[tokio::test]
async fn test_spawn_resume_path() {
    let svc = new_db().await;
    // 先建一个既有 session
    let pre = svc.create().await.unwrap();
    let sid = pre.id().to_string();
    pre.events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("第一轮")],
        })
        .await
        .unwrap();
    drop(pre);

    let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("续聊回复")]]);
    let host = make_host(svc.clone(), llm);

    let handle = host.spawn("续聊", Some(&sid)).await.unwrap();
    assert_eq!(handle.session_id, sid, "resume 应复用既有 session");
    // start = send 前可见条目数（1 条 UserMessage）
    assert_eq!(handle.message_start_index, 1);

    let output = host.send(&handle, "继续").await.unwrap();
    assert_eq!(output.text, "续聊回复");
    // 完成后可见：旧 UserMessage + 本轮 UserMessage + ModelMessage = 3 → end = 2
    assert_eq!(output.message_end_index, 2);
}

#[tokio::test]
async fn test_send_error_propagates() {
    let svc = new_db().await;
    // 非流式 error turn：stream() 返回 Err → run_tool_loop → StopReason::Error
    let llm = MockCompletionModel::from_turns([MockTurn::error("llm boom")]);
    let host = make_host(svc, llm);

    let handle = host.spawn("任务", None).await.unwrap();
    let result = host.send(&handle, "执行").await;
    assert!(
        result.is_err(),
        "LLM stream 错误应传播为 send Err: {result:?}"
    );
}

// ── 父 cancel 级联 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_parent_cancel_cascades_to_subagent() {
    let svc = new_db().await;
    let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("结果")]]);
    let parent_cancel = tokio_util::sync::CancellationToken::new();
    let host = make_host_with_cancel(svc, llm, parent_cancel.clone());

    let handle = host.spawn("任务", None).await.unwrap();

    // 父会话已取消 → send 应立即返回 Err（级联取消子代理）
    parent_cancel.cancel();
    let result = host.send(&handle, "执行").await;
    let err = result.expect_err("父 cancel 后 send 应失败");
    assert!(
        err.to_string().contains("取消"),
        "错误信息应说明取消: {err}"
    );
}

#[tokio::test]
async fn test_parent_cancel_during_send() {
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    let svc = new_db().await;
    let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("最终结果")]]);
    let parent_cancel = CancellationToken::new();

    // 子代理 run 前延迟 200ms，确保 send 进行中时触发父 cancel
    let builder: Arc<dyn AgentRunnerBuilder> = Arc::new(FakeBuilder {
        svc,
        llm,
        delay: Duration::from_millis(200),
        created: Arc::new(std::sync::Mutex::new(Vec::new())),
    });
    let host = RunnerSubagentHost::new(
        builder,
        Arc::new(TestProfile::default()),
        parent_cancel.clone(),
    );

    let handle = host.spawn("任务", None).await.unwrap();
    let send_task = tokio::spawn({
        let host = host.clone();
        async move { host.send(&handle, "执行").await }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    parent_cancel.cancel();

    let result = send_task.await.expect("send task 不应 panic");
    assert!(result.is_err(), "send 进行中父 cancel 应中断子代理");
}

// ── 子代理工具注册 ─────────────────────────────────────────────────

#[tokio::test]
async fn test_subagent_runner_tools_registered() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;

    // 未注入宿主：默认 8 个 Toolkit 工具，无 spawn_agent
    let tools = runner.build_run_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        !names.contains(&"spawn_agent"),
        "未注入宿主时不应有 spawn_agent: {names:?}"
    );
    assert_eq!(tools.len(), 8, "默认 Toolkit 工具应为 8 个: {names:?}");

    // 注入宿主（= 子代理 runner）：附加 spawn_agent，其余工具完整保留
    let builder: Arc<dyn AgentRunnerBuilder> = Arc::new(FakeBuilder {
        svc: svc.clone(),
        llm: MockCompletionModel::default(),
        delay: std::time::Duration::ZERO,
        created: Arc::new(std::sync::Mutex::new(Vec::new())),
    });
    let runner = runner.with_subagent_host_typed(builder);

    let tools = runner.build_run_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    for required in [
        "read", "edit", "write", "grep", "find", "move", "webfetch", "bash",
    ] {
        assert!(
            names.contains(&required),
            "子代理应有 {required}: {names:?}"
        );
    }
    assert!(
        names.contains(&"spawn_agent"),
        "子代理应有 spawn_agent（嵌套能力）: {names:?}"
    );
    assert_eq!(tools.len(), 9, "子代理工具应为 8 + spawn_agent: {names:?}");
}

// ── 子代理默认继承父配置 ────────────────────────────────────────────

/// 带可区分字段的测试 profile：验证子代理继承的是「父的配置」而非默认值。
#[derive(Clone)]
struct MarkerProfile {
    name: String,
    working_dir: std::path::PathBuf,
}

impl AgentProfile for MarkerProfile {
    fn with_model_profile(
        &self,
        _reducer: &dyn Fn(
            &playpen_config::model::ModelProfile,
        ) -> playpen_config::model::ModelProfile,
    ) -> Box<dyn AgentProfile> {
        Box::new(MarkerProfile {
            name: self.name.clone(),
            working_dir: self.working_dir.clone(),
        })
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        None
    }
    fn working_dir(&self) -> &std::path::PathBuf {
        &self.working_dir
    }
    fn model_profile(&self) -> &playpen_config::model::ModelProfile {
        // 用 LazyLock 持有可区分 model 的 ModelProfile
        static MP: std::sync::LazyLock<playpen_config::model::ModelProfile> =
            std::sync::LazyLock::new(|| playpen_config::model::ModelProfile {
                model: "custom-model".into(),
                temperature: Some(0.7),
                top_p: None,
                thinking_level: Some(playpen_config::model::ThinkingLevel::High),
            });
        &MP
    }
    fn instructions(&self) -> anyhow::Result<String> {
        Ok("marker instructions".into())
    }
    fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::Skill>>> {
        Ok(vec![])
    }
    fn tool_enabled(&self, _name: &str) -> bool {
        true
    }
}

#[tokio::test]
async fn test_subagent_inherits_parent_profile() {
    use std::time::Duration;

    let svc = new_db().await;
    let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("ok")]]);
    let created = Arc::new(std::sync::Mutex::new(Vec::new()));
    let builder: Arc<dyn AgentRunnerBuilder> = Arc::new(FakeBuilder {
        svc,
        llm,
        delay: Duration::ZERO,
        created: created.clone(),
    });

    // 父 profile：可区分的 name / working_dir / model
    let parent = Arc::new(MarkerProfile {
        name: "marker".into(),
        working_dir: "/custom/dir".into(),
    });
    let host = RunnerSubagentHost::new(
        builder,
        parent.clone(),
        tokio_util::sync::CancellationToken::new(),
    );

    let _handle = host.spawn("任务", None).await.unwrap();

    let recorded = created.lock().unwrap();
    assert_eq!(recorded.len(), 1, "spawn(None) 应创建一次子代理 session");
    let (name, dir, model) = &recorded[0];
    assert_eq!(name, "marker", "子代理应继承父 profile name");
    assert_eq!(dir, "/custom/dir", "子代理应继承父 working_dir");
    assert_eq!(model, "custom-model", "子代理应继承父模型配置");
}

// ── 子代理日志 ──────────────────────────────────────────────────────

/// 捕获 tracing 事件的测试订阅者（只收集 message 字段）。
struct CapturingSubscriber {
    events: Arc<std::sync::Mutex<Vec<String>>>,
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0.push_str(&format!("{value:?}"));
        }
    }
}

impl tracing::Subscriber for CapturingSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut message = String::new();
        let mut visitor = MessageVisitor(&mut message);
        event.record(&mut visitor);
        // 调试：记录 level + target + message
        let line = format!(
            "[{:?}] {} | {:?}",
            event.metadata().level(),
            event.metadata().target(),
            message
        );
        self.events.lock().unwrap().push(line);
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

/// 验证子代理日志链路（spawn/send/turn）。
///
/// `#[ignore]`：tracing 的 callsite Interest 缓存是全局的——并行测试下其他测试先执行
/// （无 dispatch），相关 callsite 首次注册被缓存为 `never`，本测试的 subscriber 收不到日志。
/// 单独运行（首次注册发生在 `with_default` 作用域内）则全部捕获：
/// `cargo test test_subagent_emits_logs -- --ignored`
#[test]
#[ignore]
fn test_subagent_emits_logs() {
    // 用显式 runtime + with_default 包裹：dispatch 在 block_on 的线程上生效，
    // 避免 #[tokio::test] 下 set_default（线程本地）与并行 worker 线程交互不可靠。
    let events = Arc::new(std::sync::Mutex::new(Vec::new()));
    let subscriber = CapturingSubscriber {
        events: events.clone(),
    };

    tracing::subscriber::with_default(subscriber, || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let svc = new_db().await;
            let llm = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("结果")]]);
            let host = make_host(svc, llm);
            let handle = host.spawn("日志测试", None).await.unwrap();
            let output = host.send(&handle, "任务").await.unwrap();
            assert_eq!(output.text, "结果");
        });
    });

    let msgs: Vec<String> = events.lock().unwrap().clone();
    let joined = msgs.join("\n");
    assert!(
        joined.contains("subagent spawn 开始"),
        "应有 spawn 开始日志:\n{joined}"
    );
    assert!(
        joined.contains("subagent spawn 完成"),
        "应有 spawn 完成日志:\n{joined}"
    );
    assert!(
        joined.contains("subagent send 开始"),
        "应有 send 开始日志:\n{joined}"
    );
    assert!(
        joined.contains("subagent turn 结束"),
        "应有 turn 结束日志:\n{joined}"
    );
    assert!(
        joined.contains("subagent send 完成"),
        "应有 send 完成日志:\n{joined}"
    );
}

#[tokio::test]
async fn test_subagent_no_text_output_returns_hint() {
    let svc = new_db().await;
    // 空轮：子代理无任何文本输出（仅工具调用或空回复）
    let llm = MockCompletionModel::from_stream_turns([Vec::new()]);
    let host = make_host(svc, llm);
    let handle = host.spawn("任务", None).await.unwrap();
    let output = host.send(&handle, "执行").await.unwrap();
    assert!(
        output.text.contains("未返回文本消息"),
        "无文本输出时应返回提示而非空串: {:?}",
        output.text
    );
}

#[tokio::test]
async fn test_subagent_tool_output_fallback() {
    let svc = new_db().await;
    // 第一轮 tool_call（产生 FunctionResult），第二轮空（无文本消息）→ 应回退工具输出
    let llm = MockCompletionModel::from_stream_turns([
        vec![MockStreamEvent::tool_call(
            "nonexistent_tool",
            "nonexistent_tool",
            serde_json::json!({}),
        )],
        Vec::new(),
    ]);
    let host = make_host(svc, llm);
    let handle = host.spawn("任务", None).await.unwrap();
    let output = host.send(&handle, "执行").await.unwrap();
    assert!(
        output.text.contains("工具输出"),
        "无文本时应回退工具输出: {:?}",
        output.text
    );
    assert!(
        output.text.contains("无效的工具"),
        "应包含工具结果文本: {:?}",
        output.text
    );
}
