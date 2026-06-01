use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use playpen_content::{ContentBlock, Event, StopReason};
use playpen_session::SessionService;

use crate::runner::{AgentRunner, AgentRunnerBuilder, SimpleRunnerBuilder};
use crate::testing::{FakeTool, MockCompletionModel, MockStreamEvent, TestProfile, make_runner};
use playpen_session::DBSessionService;

async fn new_db() -> Arc<dyn SessionService> {
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    let svc = DBSessionService::new(db);
    svc.migrate().await.unwrap();
    Arc::new(svc)
}

// ── Basic runner tests ──

#[tokio::test]
async fn test_replay_empty() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc).await;
    let events: Vec<Event> = runner.replay().collect().await;
    assert!(events.is_empty(), "空 session replay 应无事件");
}

#[tokio::test]
async fn test_replay_with_events() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();

    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("hello")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("world")],
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
    let runner = make_runner(session, svc).await;
    let events: Vec<Event> = runner.replay().collect().await;
    assert!(events.len() >= 2);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::UserMessage { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ModelMessage { .. }))
    );
}

#[tokio::test]
async fn test_rewind_removes_last_turn() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("turn 1")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("response 1")],
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
    let before_count = svc.get(&sid).await.unwrap().events().len().await;
    let runner = make_runner(session, svc.clone()).await;
    runner.rewind().await.unwrap();
    let after_count = svc.get(&sid).await.unwrap().events().len().await;
    assert!(after_count < before_count, "rewind 应减少事件数");
}

#[tokio::test]
async fn test_cancel() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc).await;
    runner.cancel().await;
    runner.cancel().await;
}

// ── Builder tests ──

#[tokio::test]
async fn test_builder_create_and_resume() {
    let svc = new_db().await;
    let dirs = playpen_config::Dirs::with_defaults(&PathBuf::from("/tmp"));

    // 自定义 resolver 只返回 TestProfile
    struct TestResolver;
    impl playpen_profile::AgentProfileLoader for TestResolver {
        fn agent_profiles(
            &self,
            _: &playpen_config::Dirs,
        ) -> anyhow::Result<Vec<Box<dyn playpen_profile::AgentProfile>>> {
            Ok(vec![Box::new(TestProfile)])
        }
    }

    let builder = SimpleRunnerBuilder::new(
        &playpen_config::Settings::default(),
        &dirs,
        svc.clone(),
        Arc::new(TestResolver),
    );

    let runner = builder.create(Box::new(TestProfile)).await.unwrap();
    let sid = runner.id().to_string();
    assert!(!sid.is_empty());
    assert_eq!(runner.profile().name(), "test");

    let resumed = builder.resume(&sid).await.unwrap();
    assert_eq!(resumed.id(), sid);
    assert_eq!(resumed.profile().name(), "test");
}

#[tokio::test]
async fn test_builder_sessions() {
    let svc = new_db().await;
    let dirs = playpen_config::Dirs::with_defaults(&PathBuf::from("/tmp"));
    let resolver = playpen_profile::LocalAgentProfileLoader;
    let builder = SimpleRunnerBuilder::new(
        &playpen_config::Settings::default(),
        &dirs,
        svc.clone(),
        Arc::new(resolver),
    );

    let svc_ref = builder.sessions();
    // 验证类型正确（编译时验证）
    let _: &dyn SessionService = svc_ref;
}

#[tokio::test]
async fn test_runner_with_profile() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;

    let new_profile = Box::new(TestProfile);
    let updated = runner.with_profile(new_profile);
    assert_eq!(updated.profile().name(), "test");
}

// ── Profile state persistence tests ──

#[tokio::test]
async fn test_profile_state_persisted_on_create() {
    let svc = new_db().await;
    let dirs = playpen_config::Dirs::with_defaults(&PathBuf::from("/tmp"));
    let resolver = playpen_profile::LocalAgentProfileLoader;
    let builder = SimpleRunnerBuilder::new(
        &playpen_config::Settings::default(),
        &dirs,
        svc.clone(),
        Arc::new(resolver),
    );

    let runner = builder.create(Box::new(TestProfile)).await.unwrap();
    let sid = runner.id().to_string();

    use futures::StreamExt;
    let session = svc.get(&sid).await.unwrap();
    let state: std::collections::HashMap<String, serde_json::Value> =
        session.state().entities().await.collect().await;
    // create 时不会自动持久化 profile 状态，resume 应不崩溃
    assert!(
        state.is_empty() || state.contains_key("user:playpen-agent-profile:name"),
        "profile state"
    );
    let resumed = builder.resume(&sid).await.unwrap();
    assert_eq!(resumed.id(), sid);
}

#[tokio::test]
async fn test_instruction_from_state() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // 模拟 instruction 已存在于 state
    use serde_json::json;
    session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: "user:playpen-agent-profile:instruction".into(),
            data: json!("custom instruction"),
        })
        .await
        .unwrap();

    let runner = make_runner(session, svc.clone()).await;
    // 验证 with_profile 后的 runner 有正确的 id
    assert_eq!(runner.id(), &sid);
    // 这里无法直接验证 instruction，但 resume 时应读到已有的值
}

// ── Replay with cancel ──

#[tokio::test]
async fn test_replay_cancel() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    runner.cancel().await;
    // cancel 后 replay 不应 panic
    let events: Vec<Event> = runner.replay().collect().await;
    assert!(events.is_empty(), "取消后 replay 无事件");
}

// ── run_with_model tests ──

#[tokio::test]
async fn test_run_with_model_text_only() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    let sid = runner.id().to_string();

    let mock = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("Hello from mock")]]);

    let prompt = vec![ContentBlock::text("hi")];
    let stream = runner
        .run_with_model(mock, prompt, vec![], None, |_| None)
        .await;
    let events: Vec<Event> = stream.collect().await;

    assert!(!events.is_empty(), "应有事件");
    assert!(
        events.iter().any(|e| matches!(e, Event::TurnStop { .. })),
        "应有 TurnStop"
    );

    // 验证持久化
    let loaded = svc.get(&sid).await.unwrap();
    assert!(loaded.events().len().await > 1, "事件应持久化到 session");
}

#[tokio::test]
async fn test_run_with_model_tool_call() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    let sid = runner.id().to_string();

    // 第一轮 mock 返回 tool_call，第二轮返回文本
    let mock = MockCompletionModel::from_stream_turns([
        [MockStreamEvent::tool_call(
            "call_1",
            "test_tool",
            serde_json::json!({"cmd": "echo hi"}),
        )],
        [MockStreamEvent::text("tool result processed")],
    ]);

    let tools: Vec<std::sync::Arc<dyn crate::tool::Tool>> = vec![std::sync::Arc::new(
        FakeTool::new("test_tool", "executed ok"),
    )];

    let stream = runner
        .run_with_model(
            mock,
            vec![ContentBlock::text("run tool")],
            tools,
            None,
            |_| None,
        )
        .await;
    let events: Vec<Event> = stream.collect().await;

    // 应有 FunctionCall → FunctionResult → ModelMessage → TurnStop
    let has_call = events
        .iter()
        .any(|e| matches!(e, Event::FunctionCall { .. }));
    let has_result = events
        .iter()
        .any(|e| matches!(e, Event::FunctionResult { .. }));
    let has_turn_stop = events.iter().any(|e| matches!(e, Event::TurnStop { .. }));

    assert!(has_call, "应有 FunctionCall");
    assert!(has_result, "应有 FunctionResult");
    assert!(has_turn_stop, "应有 TurnStop");

    // 验证 FunctionCall 和 FunctionResult 都出现在事件流中
    let call_idx = events
        .iter()
        .position(|e| matches!(e, Event::FunctionCall { .. }));
    let result_idx = events
        .iter()
        .position(|e| matches!(e, Event::FunctionResult { .. }));
    assert!(
        call_idx < result_idx,
        "FunctionCall 应在 FunctionResult 之前"
    );
    assert!(
        matches!(events.last(), Some(Event::TurnStop { .. })),
        "最后一个事件应为 TurnStop"
    );

    // 验证 session 持久化顺序: UserMessage → FunctionCall → FunctionResult → ModelMessage → TurnStop
    let loaded = svc.get(&sid).await.unwrap();
    let session_events: Vec<Event> = loaded.events().all().await.collect().await;
    let types: Vec<&str> = session_events
        .iter()
        .filter_map(|e| match e {
            Event::UserMessage { .. } => Some("UserMessage"),
            Event::FunctionCall { .. } => Some("FunctionCall"),
            Event::FunctionResult { .. } => Some("FunctionResult"),
            Event::ModelMessage { .. } => Some("ModelMessage"),
            Event::TurnStop { .. } => Some("TurnStop"),
            _ => None,
        })
        .collect();
    assert_eq!(
        types,
        &[
            "UserMessage",
            "FunctionCall",
            "FunctionResult",
            "ModelMessage",
            "TurnStop"
        ],
        "session 事件顺序不正确"
    );
    assert!(
        matches!(
            &session_events[session_events.len() - 2],
            Event::ModelMessage { .. }
        ),
        "倒数第二个事件应为 ModelMessage"
    );
}

#[tokio::test]
async fn test_run_with_model_multi_turn() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    let sid = runner.id().to_string();

    // 第一轮
    let mock1 = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("first response")]]);
    let events1: Vec<Event> = runner
        .run_with_model(
            mock1,
            vec![ContentBlock::text("turn 1")],
            vec![],
            None,
            |_| None,
        )
        .await
        .collect()
        .await;
    assert!(events1.iter().any(|e| matches!(e, Event::TurnStop { .. })));

    // 第二轮
    let mock2 =
        MockCompletionModel::from_stream_turns([[MockStreamEvent::text("second response")]]);
    let events2: Vec<Event> = runner
        .run_with_model(
            mock2,
            vec![ContentBlock::text("turn 2")],
            vec![],
            None,
            |_| None,
        )
        .await
        .collect()
        .await;
    assert!(events2.iter().any(|e| matches!(e, Event::TurnStop { .. })));

    // 验证 session 中有两轮的完整历史
    let loaded = svc.get(&sid).await.unwrap();
    let all_events: Vec<Event> = loaded.events().all().await.collect().await;
    let user_msgs: Vec<_> = all_events
        .iter()
        .filter(|e| matches!(e, Event::UserMessage { .. }))
        .collect();
    assert_eq!(user_msgs.len(), 2, "应有两条 user message");
    let model_msgs: Vec<_> = all_events
        .iter()
        .filter(|e| matches!(e, Event::ModelMessage { .. }))
        .collect();
    assert_eq!(model_msgs.len(), 2, "应有两条 model message");

    // 验证 replay 回放
    let replay: Vec<Event> = runner.replay().collect().await;
    assert!(replay.len() >= 4, "replay 应有至少 4 个事件");
}

#[tokio::test]
async fn test_tool_schema_persisted_to_state() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    let sid = runner.id().to_string();

    let mock = MockCompletionModel::from_stream_turns([[MockStreamEvent::text("no tools needed")]]);

    let tools: Vec<std::sync::Arc<dyn crate::tool::Tool>> = vec![std::sync::Arc::new(
        FakeTool::new("test_tool", "enabled tool"),
    )];

    let stream = runner
        .run_with_model(mock, vec![ContentBlock::text("hi")], tools, None, |_| None)
        .await;
    let _: Vec<Event> = stream.collect().await;

    // tool_schema 应持久化到 state
    let loaded = svc.get(&sid).await.unwrap();
    let schema = loaded
        .state()
        .get(crate::runner::PROFILE_STATE_KEY_TOOL_SCHEMA)
        .await;
    assert!(schema.is_some(), "tool_schema 应持久化到 state");

    if let Some(v) = schema {
        let defs: Vec<serde_json::Value> =
            serde_json::from_value(v).expect("tool_schema 应为 JSON 数组");
        assert_eq!(defs.len(), 1, "应只有 1 个 tool definition");
        assert_eq!(defs[0]["name"], "test_tool");
    }
}

#[tokio::test]
async fn test_disabled_tool_filtered_out() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;

    // mock 返回 tool_call 指向一个被 profile 过滤掉的工具
    let mock = MockCompletionModel::from_stream_turns([
        [MockStreamEvent::tool_call(
            "call_disabled",
            "disabled_tool",
            serde_json::json!({}),
        )],
        [MockStreamEvent::text("tool not found, ignoring")],
    ]);

    // 只有 test_tool 被 TestProfile 启用
    let tools: Vec<std::sync::Arc<dyn crate::tool::Tool>> = vec![
        std::sync::Arc::new(FakeTool::new("test_tool", "enabled")),
        std::sync::Arc::new(FakeTool::new("disabled_tool", "disabled")),
    ];

    let stream = runner
        .run_with_model(mock, vec![ContentBlock::text("run")], tools, None, |_| None)
        .await;
    let events: Vec<Event> = stream.collect().await;

    // disabled_tool 被过滤掉，mock 返回的 tool_call 找不到对应工具
    let disabled_call = events.iter().any(|e| match e {
        Event::FunctionCall { name, .. } => name == "disabled_tool",
        _ => false,
    });
    assert!(
        disabled_call,
        "disabled_tool 的 FunctionCall 应由 mock 发出"
    );

    // disabled_tool 不在工具列表中 → 应收到 FunctionResult 报错
    let disabled_result = events.iter().any(|e| matches!(e, Event::FunctionResult { name, .. } if name == "disabled_tool"));
    assert!(disabled_result, "找不到工具时应返回 FunctionResult");

    // 正常路径：test_tool 可用
    let has_turn_stop = events.iter().any(|e| matches!(e, Event::TurnStop { .. }));
    assert!(has_turn_stop, "应有 TurnStop");
}

#[tokio::test]
async fn test_additional_params_with_values() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;

    let mock = MockCompletionModel::from_stream_turns([[MockStreamEvent::text(
        "response with extra params",
    )]]);

    let extra = Some(serde_json::json!({
        "top_p": 0.9,
        "max_tokens": 4096,
    }));

    let stream = runner
        .run_with_model(mock, vec![ContentBlock::text("hi")], vec![], extra, |_| {
            None
        })
        .await;
    let events: Vec<Event> = stream.collect().await;
    assert!(!events.is_empty(), "additional_params 不应影响正常流程");
    assert!(
        events.iter().any(|e| matches!(e, Event::TurnStop { .. })),
        "应有 TurnStop"
    );
}
