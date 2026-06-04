//! PromptRequest 处理：核心 Agent 循环 + 流式通知。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agent_client_protocol::role::acp::Client;
use agent_client_protocol::schema::{
    CancelNotification, ContentBlock, PromptRequest, PromptResponse, SessionNotification,
    SessionUpdate,
};
use agent_client_protocol::util;
use agent_client_protocol::{ConnectionTo, Responder};
use playpen_agent_core::agent::runner::{run_agent_stream, AgentEvent};

use crate::agent::AcpState;
use crate::event_mapping;

/// 简易 Drop guard，用于在离开作用域时执行清理。
struct Cleanup<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Drop for Cleanup<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f();
        }
    }
}

/// 从 PromptRequest 的 content blocks 中拼接用户文本。
fn extract_text(blocks: &[ContentBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            ContentBlock::Text(tc) => parts.push(tc.text.clone()),
            ContentBlock::ResourceLink(rl) => {
                parts.push(format!("[资源链接: {}]", rl.uri));
            }
            _ => {}
        }
    }
    parts.join("\n")
}

/// 处理 `session/prompt` 请求。
pub async fn handle_prompt(
    req: PromptRequest,
    responder: Responder<PromptResponse>,
    cx: ConnectionTo<Client>,
    state: Arc<AcpState>,
) -> Result<(), agent_client_protocol::Error> {
    let user_input = extract_text(&req.prompt);
    let session_id = req.session_id.to_string();

    tracing::info!(
        session_id = %session_id,
        input_len = user_input.len(),
        "收到 prompt 请求"
    );

    // 获取 session
    let session = state.session_manager.get(&session_id).ok_or_else(|| {
        util::internal_error(format!("session 不存在: {}", session_id))
    })?;

    // 创建取消标记
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut cancel_map = state.cancel_flags.lock().unwrap();
        cancel_map.insert(session_id.clone(), cancel_flag.clone());
    }

    let cancel_map_ref = state.cancel_flags.clone();
    let sid_cleanup = session_id.clone();
    let _cleanup = Cleanup(Some(Box::new(move || {
        if let Ok(mut map) = cancel_map_ref.lock() {
            map.remove(&sid_cleanup);
        }
    })));

    // 获取事件流：优先使用 mock，否则走真实 run_agent_stream
    let mut event_rx = if let Some(rx) = state.mock_event_rx.lock().await.take() {
        rx
    } else {
        // 构建 CompletionsClient
        let provider_id = super::session::resolve_provider_for_model(&state, &session.model.id)
            .unwrap_or_else(|| "unknown".to_string());
        let client = state.registry.build_client(&provider_id).map_err(|e| {
            util::internal_error(format!("构建 client 失败: {e}"))
        })?;

        // 构建 tools
        let tools = build_tools(&state);

        run_agent_stream(
            &client,
            &session,
            &user_input,
            tools,
            None, // memory 暂不启用
            cancel_flag.clone(),
        )
    };

    // 消费事件并发送通知
    while let Some(event) = event_rx.recv().await {
        if cancel_flag.load(Ordering::Relaxed) {
            tracing::info!(session_id = %session_id, "prompt 已取消");
            let response =
                PromptResponse::new(agent_client_protocol::schema::StopReason::Cancelled);
            responder.respond(response)?;
            return Ok(());
        }

        match event {
            AgentEvent::Done { stop_reason, .. } => {
                let acp_stop = event_mapping::map_stop_reason(&stop_reason);
                tracing::info!(session_id = %session_id, stop_reason = ?acp_stop, "prompt 完成");
                state.session_store.persist(&session_id);
                responder.respond(PromptResponse::new(acp_stop))?;
                return Ok(());
            }
            AgentEvent::Error(msg) => {
                tracing::error!(session_id = %session_id, error = %msg, "prompt 出错");
                let err_chunk = agent_client_protocol::schema::ContentChunk::new(
                    ContentBlock::Text(
                        agent_client_protocol::schema::TextContent::new(format!("错误: {msg}")),
                    ),
                );
                let err_notif = SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(err_chunk),
                );
                let _ = cx.send_notification(err_notif);
                state.session_store.persist(&session_id);
                return Err(util::internal_error(msg));
            }
            AgentEvent::ToolCallStart { .. } => {
                // 背靠背：先发 ToolCall(Pending)，再发 ToolCallUpdate(InProgress)
                if let Some(pending) = event_mapping::to_session_update(&event) {
                    cx.send_notification(SessionNotification::new(
                        session_id.clone(),
                        pending,
                    ))
                    .map_err(|e| util::internal_error(format!("发送通知失败: {e}")))?;
                }
                if let Some(in_progress) = event_mapping::to_tool_call_in_progress(&event) {
                    cx.send_notification(SessionNotification::new(
                        session_id.clone(),
                        in_progress,
                    ))
                    .map_err(|e| util::internal_error(format!("发送通知失败: {e}")))?;
                }
            }
            other => {
                if let Some(update) = event_mapping::to_session_update(&other) {
                    cx.send_notification(SessionNotification::new(
                        session_id.clone(),
                        update,
                    ))
                    .map_err(|e| util::internal_error(format!("发送通知失败: {e}")))?;
                }
            }
        }
    }

    // channel 关闭但未收到 Done
    tracing::warn!(session_id = %session_id, "事件通道异常关闭");
    state.session_store.persist(&session_id);
    responder.respond(PromptResponse::new(
        agent_client_protocol::schema::StopReason::EndTurn,
    ))?;
    Ok(())
}

fn build_tools(state: &AcpState) -> Vec<Box<dyn rig_core::tool::ToolDyn>> {
    use playpen_agent_core::tools::{
        bash::BashRigTool, edit::EditRigTool, find::FindRigTool, grep::GrepRigTool,
        r#move::MoveRigTool, read::ReadRigTool, webfetch::WebfetchRigTool, write::WriteRigTool,
    };
    let ws = state.workspace.clone();
    vec![
        Box::new(ReadRigTool { ws: ws.clone() }),
        Box::new(GrepRigTool { ws: ws.clone() }),
        Box::new(EditRigTool { ws: ws.clone() }),
        Box::new(WriteRigTool { ws: ws.clone() }),
        Box::new(MoveRigTool { ws: ws.clone() }),
        Box::new(FindRigTool { ws: ws.clone() }),
        Box::new(BashRigTool { ws: ws.clone() }),
        Box::new(WebfetchRigTool),
    ]
}

/// 处理 `session/cancel` 通知。
pub async fn handle_cancel_notification(
    notif: CancelNotification,
    _cx: ConnectionTo<Client>,
    state: Arc<AcpState>,
) -> Result<(), agent_client_protocol::Error> {
    let session_id = notif.session_id.to_string();
    tracing::info!(session_id = %session_id, "收到 cancel 通知");

    if let Ok(map) = state.cancel_flags.lock()
        && let Some(flag) = map.get(&session_id) {
            flag.store(true, Ordering::Relaxed);
            tracing::info!(session_id = %session_id, "已设置取消标记");
        }

    Ok(())
}
