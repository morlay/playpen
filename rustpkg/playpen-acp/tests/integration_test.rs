//! playpen-acp 端到端集成测试。
//!
//! 通过 ACP Channel duplex 模拟 Client ↔ Agent 通信流。

use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::schema::{
    ContentBlock, InitializeRequest, InitializeResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, ProtocolVersion, StopReason, TextContent,
};
use agent_client_protocol::{Channel, Client};
use playpen_agent_core::agent::runner::AgentEvent;
use serde_json::json;
use tokio::sync::mpsc;

use playpen_acp::agent::{self, AcpState};

/// 启动 agent 端（使用 channel），返回 JoinHandle
fn spawn_agent(channel: Channel, state: Arc<AcpState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = agent::run_with_transport(state, channel).await {
            eprintln!("Agent 退出: {e:?}");
        }
    })
}

// ---- 测试：Initialize → NewSession → Prompt 完整流程 ----

#[tokio::test(flavor = "multi_thread")]
async fn test_full_prompt_flow() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let state = agent::build_state(&cwd).expect("构建 AcpState 失败");

    // 注入 mock 事件流
    let (tx, rx) = mpsc::unbounded_channel();
    *state.mock_event_rx.lock().await = Some(rx);

    // 预先写入 mock 事件
    tx.send(AgentEvent::TextDelta("Hello ".into())).unwrap();
    tx.send(AgentEvent::TextDelta("World!".into())).unwrap();
    tx.send(AgentEvent::Done {
        message: playpen_agent_core::session::message::Message::assistant("Hello World!"),
        usage: Default::default(),
        stop_reason: playpen_agent_core::model::StopReason::Stop,
    }).unwrap();

    let (agent_chan, client_chan) = Channel::duplex();
    spawn_agent(agent_chan, state);

    // Client 端
    let result = Client.builder()
        .name("test-client")
        .connect_with(client_chan, async |cx| {
            // Step 1: Initialize
            let init_resp: InitializeResponse = cx
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            assert!(init_resp.agent_info.is_some());

            // Step 2: NewSession
            let meta = Some({
                let mut m = serde_json::Map::new();
                m.insert("_playpen".into(), json!({
                    "agent_name": "default",
                    "model_id": "deepseek-v4-flash",
                }));
                m
            });
            let new_session_resp: NewSessionResponse = cx
                .send_request(
                    NewSessionRequest::new(".")
                        .additional_directories(vec![])
                        .meta(meta),
                )
                .block_task()
                .await?;
            let session_id = new_session_resp.session_id.clone();
            assert!(!session_id.to_string().is_empty());

            // Step 3: Prompt
            let prompt_resp: PromptResponse = cx
                .send_request(PromptRequest::new(
                    session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new("hi"))],
                ))
                .block_task()
                .await?;
            assert_eq!(prompt_resp.stop_reason, StopReason::EndTurn);

            Ok(())
        })
        .await;

    assert!(result.is_ok(), "Client 流程应成功: {:?}", result.err());
}

// ---- 测试：ToolCall 背靠背发送 ----

#[tokio::test(flavor = "multi_thread")]
async fn test_tool_call_two_phase() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let state = agent::build_state(&cwd).expect("构建 AcpState 失败");

    let (tx, rx) = mpsc::unbounded_channel();
    *state.mock_event_rx.lock().await = Some(rx);

    tx.send(AgentEvent::ToolCallStart {
        id: "tc1".into(),
        name: "bash".into(),
        arguments: r#"{"cmd":"ls"}"#.into(),
    }).unwrap();
    tx.send(AgentEvent::Done {
        message: playpen_agent_core::session::message::Message::assistant("done"),
        usage: Default::default(),
        stop_reason: playpen_agent_core::model::StopReason::ToolUse,
    }).unwrap();

    let (agent_chan, client_chan) = Channel::duplex();
    spawn_agent(agent_chan, state);

    let result = Client.builder()
        .name("test-client")
        .connect_with(client_chan, async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;

            let meta = Some({
                let mut m = serde_json::Map::new();
                m.insert("_playpen".into(), json!({
                    "agent_name": "default",
                    "model_id": "deepseek-v4-flash",
                }));
                m
            });
            let ns: NewSessionResponse = cx
                .send_request(NewSessionRequest::new(".").meta(meta))
                .block_task()
                .await?;

            let resp: PromptResponse = cx
                .send_request(PromptRequest::new(
                    ns.session_id,
                    vec![ContentBlock::Text(TextContent::new("test"))],
                ))
                .block_task()
                .await?;
            assert_eq!(resp.stop_reason, StopReason::EndTurn);

            Ok(())
        })
        .await;

    assert!(result.is_ok(), "ToolCall 测试应成功: {:?}", result.err());
}
