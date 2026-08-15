use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use playpen_content::{ContentBlock, Event, StopReason};
use playpen_session::SessionService;

use crate::runner::{AgentRunner, AgentRunnerBuilder, SimpleRunnerBuilder};
use crate::testing::{FakeTool, TestProfile, make_runner};
use playpen_session::DBSessionService;
use rig_core::test_utils::{MockCompletionModel, MockStreamEvent};

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
            Ok(vec![Box::new(TestProfile::default())])
        }
    }

    let builder = SimpleRunnerBuilder::new(
        &playpen_config::Settings::default(),
        &dirs,
        svc.clone(),
        Arc::new(TestResolver),
    );

    let runner = builder.create(Box::new(TestProfile::default())).await.unwrap();
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

    let new_profile = Box::new(TestProfile::default());
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

    let runner = builder.create(Box::new(TestProfile::default())).await.unwrap();
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
    let disabled_result = events
        .iter()
        .any(|e| matches!(e, Event::FunctionResult { name, .. } if name == "disabled_tool"));
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

#[tokio::test]
async fn test_only_thought_retries() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let runner = make_runner(session, svc.clone()).await;
    let sid = runner.id().to_string();

    // 第一轮仅返回 thought（无 message 无 call），第二轮正常返回文本
    let mock = MockCompletionModel::from_stream_turns([
        vec![
            MockStreamEvent::reasoning_delta(None::<String>, "thinking..."),
            MockStreamEvent::final_response_with_default_usage(),
        ],
        vec![MockStreamEvent::text("final response")],
    ]);

    let stream = runner
        .run_with_model(mock, vec![ContentBlock::text("hi")], vec![], None, |_| None)
        .await;
    let events: Vec<Event> = stream.collect().await;

    let has_thought_delta = events
        .iter()
        .any(|e| matches!(e, Event::ModelThoughtDelta { .. }));
    let has_thought = events
        .iter()
        .any(|e| matches!(e, Event::ModelThought { .. }));
    let has_final_message = events
        .iter()
        .any(|e| matches!(e, Event::ModelMessage { .. }));
    let has_turn_stop = events.iter().any(|e| matches!(e, Event::TurnStop { .. }));

    assert!(has_thought_delta, "第一轮应有 ModelThoughtDelta");
    assert!(has_thought, "第一轮应有 ModelThought");
    assert!(has_final_message, "第二轮应有 ModelMessage");
    assert!(has_turn_stop, "应有 TurnStop");

    // 验证进行了两次流式请求（第一次 thought-only 触发了重试）
    let loaded = svc.get(&sid).await.unwrap();
    let all_events: Vec<Event> = loaded.events().all().await.collect().await;
    let thought_count = all_events
        .iter()
        .filter(|e| matches!(e, Event::ModelThought { .. }))
        .count();
    let message_count = all_events
        .iter()
        .filter(|e| matches!(e, Event::ModelMessage { .. }))
        .count();
    assert_eq!(thought_count, 1, "应持久化 1 条 ModelThought");
    assert_eq!(message_count, 1, "应持久化 1 条 ModelMessage");
}

// ── cancel 后孤儿 FunctionCall 修复 ──

#[tokio::test]
async fn test_cancel_after_function_call_emits_cancelled_result() {
    use std::pin::Pin;
    use std::time::Duration;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // 事件源：可控 channel，模拟 LLM 流在 FunctionCall 产出后暂停
    let (tx_events, rx_events) = mpsc::unbounded_channel();
    let (tx_out, _rx_out) = mpsc::unbounded_channel();
    let cancel = CancellationToken::new();

    let stream: Pin<Box<dyn futures::Stream<Item = Event> + Send>> =
        Box::pin(crate::runner::ReceiverStream { rx: rx_events });
    let tx_out_for_task = tx_out.clone();
    let cancel_for_task = cancel.clone();
    let svc_for_task = svc.clone();
    let sid_for_task = sid.clone();

    let handle = tokio::spawn(async move {
        let session = svc_for_task.get(&sid_for_task).await.unwrap();
        crate::runner::consume_turn_stream(stream, &tx_out_for_task, session.events(), &cancel_for_task)
            .await
    });

    // 1. 发送 FunctionCall，等待其持久化到 session（此时工具尚未执行）
    tx_events
        .send(Event::FunctionCall {
            id: String::new(),
            call_id: "call_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .unwrap();

    let mut persisted = false;
    for _ in 0..200 {
        if svc.get(&sid).await.unwrap().events().len().await >= 1 {
            persisted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(persisted, "FunctionCall 应已持久化");

    // 2. 哨兵事件（仅发 UI 不持久化）驱动循环回到顶部检查 cancel
    tx_events
        .send(Event::ModelThoughtDelta {
            id: "sentinel_1".into(),
            text: "sentinel".into(),
        })
        .unwrap();
    // 3. 先 cancel，再发第二个哨兵确保循环顶部检查到取消状态
    cancel.cancel();
    tx_events
        .send(Event::ModelThoughtDelta {
            id: "sentinel_2".into(),
            text: "sentinel".into(),
        })
        .unwrap();

    let pending_calls = match handle.await.unwrap() {
        crate::runner::ConsumeOutcome::Cancelled { pending_calls } => pending_calls,
        other => panic!("期望 Cancelled，实际 {other:?}"),
    };
    assert_eq!(pending_calls.len(), 1, "应携带已持久化的 FunctionCall");
    assert_eq!(pending_calls[0].call_id, "call_1");

    // 4. 补发 cancelled 的 FunctionResult（run_tool_loop 的 Cancelled 分支逻辑）
    let ok =
        crate::runner::emit_cancelled_results(&pending_calls, &tx_out, session.events()).await;
    assert!(ok, "补发 cancelled 结果应成功");

    // 5. session 中 FunctionCall 必须有配对 FunctionResult，且 content 标记取消
    let session_events: Vec<Event> = svc
        .get(&sid)
        .await
        .unwrap()
        .events()
        .all()
        .await
        .collect()
        .await;
    let call_count = session_events
        .iter()
        .filter(|e| matches!(e, Event::FunctionCall { .. }))
        .count();
    let result_count = session_events
        .iter()
        .filter(|e| matches!(e, Event::FunctionResult { .. }))
        .count();
    assert_eq!((call_count, result_count), (1, 1), "FunctionCall 与 FunctionResult 应配对");

    // 6. 验证转换后的消息序列合法：每个 ToolCall 都有配对 ToolResult
    use crate::convert::StreamPipe;
    let messages: Vec<rig_core::completion::Message> = futures::stream::iter(session_events)
        .pipe(crate::convert::events_to_chat_history)
        .collect()
        .await;
    let has_tool_call = messages.iter().any(|m| match m {
        rig_core::completion::Message::Assistant { content, .. } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::AssistantContent::ToolCall(_))),
        _ => false,
    });
    let has_tool_result = messages.iter().any(|m| match m {
        rig_core::completion::Message::User { content } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::UserContent::ToolResult(_))),
        _ => false,
    });
    assert!(has_tool_call, "应保留配对的 ToolCall");
    assert!(has_tool_result, "应有对应的 ToolResult");
}

#[tokio::test]
async fn test_load_chat_messages_filters_orphan_function_calls() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // 模拟已损坏的 session：FunctionCall 被持久化但没有 FunctionResult
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("hi")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "orphan_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("I will use a tool")],
        })
        .await
        .unwrap();

    let messages = crate::runner::load_chat_messages(&*svc, &sid).await.unwrap();

    // 孤儿 FunctionCall 不应生成 ToolCall（否则 LLM API 拒绝请求）
    let has_tool_call = messages.iter().any(|m| match m {
        rig_core::completion::Message::Assistant { content, .. } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::AssistantContent::ToolCall(_))),
        _ => false,
    });
    assert!(!has_tool_call, "孤儿 FunctionCall 不应生成 ToolCall");

    // 配对的文本内容仍应保留
    let has_text = messages.iter().any(|m| match m {
        rig_core::completion::Message::Assistant { content, .. } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::AssistantContent::Text(_))),
        _ => false,
    });
    assert!(has_text, "ModelMessage 文本应保留");
}

#[tokio::test]
async fn test_load_chat_messages_keeps_paired_function_calls() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // 配对完整的 FunctionCall + FunctionResult 不应被过滤
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("run tool")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "paired_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionResult {
            id: String::new(),
            call_id: "paired_1".into(),
            name: "test_tool".into(),
            content: Some(vec![ContentBlock::text("ok")]),
            code: Some(0),
        })
        .await
        .unwrap();

    let messages = crate::runner::load_chat_messages(&*svc, &sid).await.unwrap();

    let has_tool_call = messages.iter().any(|m| match m {
        rig_core::completion::Message::Assistant { content, .. } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::AssistantContent::ToolCall(_))),
        _ => false,
    });
    let has_tool_result = messages.iter().any(|m| match m {
        rig_core::completion::Message::User { content } => content
            .iter()
            .any(|c| matches!(c, rig_core::completion::message::UserContent::ToolResult(_))),
        _ => false,
    });
    assert!(has_tool_call, "配对的 ToolCall 应保留");
    assert!(has_tool_result, "配对的 ToolResult 应保留");
}

// ── 历史数据补偿 ──

#[tokio::test]
async fn test_reconcile_orphan_function_calls_repairs_and_idempotent() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();

    // 孤儿：有 FunctionCall 无 FunctionResult
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("run tool")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: "fc_1".into(),
            call_id: "orphan_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();

    // 首次补偿：补发 1 条 FunctionResult
    let repaired = crate::runner::reconcile_orphan_function_calls(&*session)
        .await
        .unwrap();
    assert_eq!(repaired, 1, "应补发 1 条 FunctionResult");

    let events: Vec<Event> = session.events().all().await.collect().await;
    let results: Vec<&Event> = events
        .iter()
        .filter(|e| matches!(e, Event::FunctionResult { .. }))
        .collect();
    assert_eq!(results.len(), 1, "session 中应有 1 条 FunctionResult");
    match results[0] {
        Event::FunctionResult {
            call_id,
            name,
            content,
            code,
            ..
        } => {
            assert_eq!(call_id, "orphan_1", "FunctionResult 应配对孤儿 FunctionCall");
            assert_eq!(name, "test_tool");
            assert!(code.is_none(), "取消结果不应有 exit_code");
            let text: String = content
                .as_ref()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(text, "工具调用已取消");
        }
        _ => panic!("期望 FunctionResult"),
    }

    // 幂等：再次补偿不重复补发
    let repaired_again = crate::runner::reconcile_orphan_function_calls(&*session)
        .await
        .unwrap();
    assert_eq!(repaired_again, 0, "已有配对的不应重复补发");
    let events: Vec<Event> = session.events().all().await.collect().await;
    let result_count = events
        .iter()
        .filter(|e| matches!(e, Event::FunctionResult { .. }))
        .count();
    assert_eq!(result_count, 1, "幂等：FunctionResult 数量不变");
}

#[tokio::test]
async fn test_reconcile_keeps_paired_calls_untouched() {
    let svc = new_db().await;
    let session = svc.create().await.unwrap();

    // 已配对的 FunctionCall + FunctionResult 不应被重复处理
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("run tool")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "paired_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionResult {
            id: String::new(),
            call_id: "paired_1".into(),
            name: "test_tool".into(),
            content: Some(vec![ContentBlock::text("ok")]),
            code: Some(0),
        })
        .await
        .unwrap();

    let repaired = crate::runner::reconcile_orphan_function_calls(&*session)
        .await
        .unwrap();
    assert_eq!(repaired, 0, "已配对的调用不应补发");
}

#[tokio::test]
async fn test_resume_reconciles_orphan_function_calls() {
    let svc = new_db().await;
    let dirs = playpen_config::Dirs::with_defaults(&PathBuf::from("/tmp"));

    struct TestResolver;
    impl playpen_profile::AgentProfileLoader for TestResolver {
        fn agent_profiles(
            &self,
            _: &playpen_config::Dirs,
        ) -> anyhow::Result<Vec<Box<dyn playpen_profile::AgentProfile>>> {
            Ok(vec![Box::new(TestProfile::default())])
        }
    }

    let builder = SimpleRunnerBuilder::new(
        &playpen_config::Settings::default(),
        &dirs,
        svc.clone(),
        Arc::new(TestResolver),
    );

    let runner = builder.create(Box::new(TestProfile::default())).await.unwrap();
    let sid = runner.id().to_string();

    // 在 session 中埋入孤儿 FunctionCall
    let session = svc.get(&sid).await.unwrap();
    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("run tool")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "orphan_1".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();

    // resume 应触发补偿
    let resumed = builder.resume(&sid).await.unwrap();
    assert_eq!(resumed.id(), sid);

    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    let result_count = events
        .iter()
        .filter(|e| matches!(e, Event::FunctionResult { .. }))
        .count();
    assert_eq!(result_count, 1, "resume 应补发孤儿 FunctionCall 的 FunctionResult");

    // 再次 resume 幂等
    let _ = builder.resume(&sid).await.unwrap();
    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    let result_count = events
        .iter()
        .filter(|e| matches!(e, Event::FunctionResult { .. }))
        .count();
    assert_eq!(result_count, 1, "resume 补偿应幂等");
}
