use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::Stream;
use playpen_content::{ContentBlock, Event};
use playpen_profile::AgentProfile;
use playpen_session::Session;
use serde_json::json;
use tokio::sync::mpsc;

use crate::runner::{AgentRunner, AgentRunnerBuilder};
use crate::subagent::{SubagentHandle, SubagentHost, SubagentOutput};
use crate::tool::{SpawnAgentTool, Tool, ToolContext};

// ── FakeRunner ──────────────────────────────────────────────────────

#[derive(Clone)]
struct FakeRunner {
    id: String,
}

#[async_trait]
impl AgentRunner for FakeRunner {
    fn id(&self) -> &str {
        &self.id
    }
    fn session(&self) -> &dyn Session {
        unimplemented!("工具测试不使用 session")
    }
    fn profile(&self) -> &dyn AgentProfile {
        unimplemented!("工具测试不使用 profile")
    }
    fn settings(&self) -> &playpen_config::Settings {
        unimplemented!("工具测试不使用 settings")
    }
    fn with_profile(&self, _p: Box<dyn AgentProfile>) -> Box<dyn AgentRunner> {
        Box::new(self.clone())
    }
    fn with_subagent_host(&self, _b: Arc<dyn AgentRunnerBuilder>) -> Box<dyn AgentRunner> {
        Box::new(self.clone())
    }
    async fn run(&self, _prompt: Vec<ContentBlock>) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        Box::pin(futures::stream::empty())
    }
    async fn rewind(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn replay(&self) -> Pin<Box<dyn Stream<Item = Event> + Send>> {
        Box::pin(futures::stream::empty())
    }
    async fn cancel(&self) {}
}

// ── FakeHost ────────────────────────────────────────────────────────

struct FakeHost {
    output: String,
    send_error: Option<String>,
    spawn_error: Option<String>,
    /// 记录 spawn 调用：(label, session_id)
    spawns: Mutex<Vec<(String, Option<String>)>>,
}

impl FakeHost {
    fn ok(output: &str) -> Self {
        Self {
            output: output.into(),
            send_error: None,
            spawn_error: None,
            spawns: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl SubagentHost for FakeHost {
    async fn spawn(&self, label: &str, session_id: Option<&str>) -> anyhow::Result<SubagentHandle> {
        self.spawns
            .lock()
            .unwrap()
            .push((label.to_string(), session_id.map(|s| s.to_string())));
        if let Some(e) = &self.spawn_error {
            return Err(anyhow::anyhow!(e.to_string()));
        }
        let runner: Box<dyn AgentRunner> = Box::new(FakeRunner {
            id: "sub-001".into(),
        });
        Ok(SubagentHandle::new(runner, 0))
    }

    async fn send(
        &self,
        _handle: &SubagentHandle,
        _prompt: &str,
    ) -> anyhow::Result<SubagentOutput> {
        if let Some(e) = &self.send_error {
            return Err(anyhow::anyhow!(e.to_string()));
        }
        Ok(SubagentOutput {
            text: self.output.clone(),
            message_end_index: 3,
        })
    }
}

// ── helpers ─────────────────────────────────────────────────────────

async fn run_tool(
    host: Arc<dyn SubagentHost>,
    args: serde_json::Value,
) -> (Vec<ContentBlock>, Vec<Event>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let cancel = tokio_util::sync::CancellationToken::new();
    let ctx = ToolContext::new("evt-1", "call-1", "spawn_agent", tx, cancel);
    let tool = SpawnAgentTool::new(host);
    let blocks = tool
        .execute(ctx, args)
        .await
        .expect("execute 不应 Err（铁律）");
    let mut events = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        events.push(ev);
    }
    (blocks, events)
}

fn text_of(blocks: &[ContentBlock]) -> Option<String> {
    blocks.iter().find_map(|b| match b {
        ContentBlock::Text(t) => Some(t.text.clone()),
        _ => None,
    })
}

fn annotations_of(blocks: &[ContentBlock]) -> Option<&serde_json::Value> {
    blocks.iter().find_map(|b| match b {
        ContentBlock::Text(t) => t.annotations.as_ref(),
        _ => None,
    })
}

// ── tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_spawn_success_carries_session_info() {
    let host: Arc<dyn SubagentHost> = Arc::new(FakeHost::ok("子任务完成，结果是 42"));
    let (blocks, events) =
        run_tool(host, json!({ "label": "计算", "message": "请计算 40+2" })).await;

    let text = text_of(&blocks).expect("成功应有输出文本");
    assert_eq!(text, "子任务完成，结果是 42");

    let ann = annotations_of(&blocks).expect("成功结果应带 annotations");
    assert_eq!(ann["exit_code"], 0);
    let info = &ann["_meta.subagent_session_info"];
    assert_eq!(info["session_id"], "sub-001");
    assert_eq!(info["message_start_index"], 0);
    assert_eq!(info["message_end_index"], 3);

    // 运行中应发射一条 FunctionOutputDelta 进度提示
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::FunctionOutputDelta { .. })),
        "创建子代理后应发射进度 delta"
    );
}

#[tokio::test]
async fn test_send_failure_keeps_session_id() {
    let mut host = FakeHost::ok("unused");
    host.send_error = Some("子代理 LLM 出错".into());
    let host: Arc<dyn SubagentHost> = Arc::new(host);

    let (blocks, _) = run_tool(host, json!({ "label": "调研", "message": "调研一下" })).await;

    let text = text_of(&blocks).expect("失败也应有输出文本");
    assert!(
        text.contains("子代理执行失败"),
        "失败文本应说明原因: {text}"
    );

    let ann = annotations_of(&blocks).expect("失败结果也应带 annotations");
    // 铁律：失败走 Ok + exit_code=1，annotations 保留 session_id
    assert_eq!(ann["exit_code"], 1);
    assert_eq!(ann["_meta.subagent_session_info"]["session_id"], "sub-001");
    // 失败时 message_end_index 为 null（Zed 端 Option 兼容）
    assert!(ann["_meta.subagent_session_info"]["message_end_index"].is_null());
}

#[tokio::test]
async fn test_spawn_failure_returns_err() {
    let mut host = FakeHost::ok("unused");
    host.spawn_error = Some("创建 session 失败".into());
    let host: Arc<dyn SubagentHost> = Arc::new(host);

    let (tx, _rx) = mpsc::unbounded_channel();
    let cancel = tokio_util::sync::CancellationToken::new();
    let ctx = ToolContext::new("evt-1", "call-1", "spawn_agent", tx, cancel);
    let tool = SpawnAgentTool::new(host);

    let result = tool
        .execute(ctx, json!({ "label": "x", "message": "y" }))
        .await;
    assert!(
        result.is_err(),
        "spawn 失败应返回 Err（无 session 信息可携带）"
    );
}

#[tokio::test]
async fn test_resume_path_forwards_session_id() {
    let host = Arc::new(FakeHost::ok("续聊回复"));
    let (_, _) = run_tool(
        host.clone(),
        json!({ "label": "续聊", "message": "继续", "session_id": "sub-009" }),
    )
    .await;

    let spawns = host.spawns.lock().unwrap();
    assert_eq!(spawns.len(), 1);
    assert_eq!(spawns[0].0, "续聊");
    assert_eq!(spawns[0].1.as_deref(), Some("sub-009"));
}
