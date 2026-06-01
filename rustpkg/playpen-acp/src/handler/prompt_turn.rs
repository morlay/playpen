use std::sync::Arc;

use agent_client_protocol::Responder;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock as AcpContentBlock, ContentChunk, PromptRequest,
    PromptResponse, SessionUpdate, StopReason as AcpStopReason, Usage as AcpUsage,
};
use futures::StreamExt;

use crate::acp_content::to_agent_blocks;
use crate::acp_state::Context;
use crate::event_mapper::EventMapper;
use crate::handler::session_setup::apply_pending_config;
use crate::slash_command;

/// 在 dispatch loop 中快速执行，仅做注册和 spawn，不阻塞事件循环
pub(crate) async fn handle_prompt(
    ctx: crate::acp_state::AcpStateContext,
    req: PromptRequest,
    responder: Responder<PromptResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let sid = req.session_id.to_string();

    // 读取 pending config（由 set_config_option 缓存的配置变更），常驻不删除
    let pending = ctx.get_pending_config(&sid);

    // 单次 resume：如有 pending 配置则应用并返回新 runner，避免二次 resume 丢失配置变更
    let runner = if let Some(ref c) = pending {
        let r = ctx.builder().resume(&sid).await.map_err(|e| {
            agent_client_protocol::util::internal_error(format!("session {sid} 未就绪: {e}"))
        })?;

        apply_pending_config(c, r)
    } else {
        ctx.builder().resume(&sid).await.map_err(|e| {
            agent_client_protocol::util::internal_error(format!("session {sid} 未就绪: {e}"))
        })?
    };

    let runner = Arc::new(runner);
    ctx.register_running_runner(&sid, runner.clone()).await;

    // 将事件流处理移到后台任务，dispatch loop 不被阻塞，cancel 通知可正常路由
    tokio::spawn(async move {
        run_prompt_loop(ctx, sid, req, responder, runner).await;
    });

    Ok(())
}

/// 在独立的后台任务中执行 prompt 事件流处理，不阻塞 dispatch loop。
/// runner 的 Arc 由 spawn 传入，函数结束后 Arc drop → Weak 自动过期。
/// 所有错误通过 `ctx.notify_error` + `responder.respond` 通知 client，避免 client 挂等。
async fn run_prompt_loop(
    ctx: crate::acp_state::AcpStateContext,
    sid: String,
    req: PromptRequest,
    responder: Responder<PromptResponse>,
    runner: Arc<Box<dyn playpen_agent::AgentRunner>>,
) {
    let agent_profile: &dyn playpen_profile::AgentProfile = runner.profile();
    let skills = agent_profile.available_skills().unwrap_or_default();
    let (processed_blocks, rewind_requested) =
        slash_command::process_slash_commands(req.prompt, &skills);

    if rewind_requested && let Err(e) = runner.rewind().await {
        tracing::warn!(session_id = %sid, error = %e, "rewind 失败");
    }

    let prompt_blocks = to_agent_blocks(&processed_blocks);
    let message_id = uuid::Uuid::now_v7().to_string();

    // 仅在 rewind 时回显用户消息并提示上一条已废弃
    if rewind_requested
        && let Err(e) = send_echo_notifications(&ctx, &sid, &processed_blocks, &message_id) {
            tracing::warn!(session_id = %sid, error = %e, "send echo notifications 失败");
        }

    // 纯 /rewind 无后续内容 → 仅回滚，不执行 prompt
    let pure_rewind = rewind_requested && prompt_blocks.is_empty();
    if pure_rewind {
        tracing::info!(session_id = %sid, "纯 rewind，跳过 prompt");
        let _ = responder.respond(PromptResponse::new(AcpStopReason::EndTurn));
        return;
    }

    tracing::info!(session_id = %sid, model_profile = ?agent_profile.model_profile(), "running");

    let term_enabled = ctx.has_flag("terminal_output");
    let project_root = agent_profile.working_dir().clone();

    let model = runner
        .settings()
        .find_model(&agent_profile.model_profile().model);

    let mut stream = runner.run(prompt_blocks).await;

    let mut last_stop_reason = AcpStopReason::EndTurn;
    let mut last_token_usage: Option<playpen_content::TokenUsage> = None;

    while let Some(event) = stream.next().await {
        if let playpen_content::Event::TurnStop {
            stop_reason,
            token_usage,
            ..
        } = &event
        {
            last_token_usage = token_usage.clone();
            last_stop_reason = match stop_reason {
                playpen_content::StopReason::Cancelled => AcpStopReason::Cancelled,
                playpen_content::StopReason::Refusal => AcpStopReason::Refusal,
                playpen_content::StopReason::Error(_) => AcpStopReason::Refusal,
                _ => AcpStopReason::EndTurn,
            };
        }

        let mapper = EventMapper::new(&project_root)
            .with_term_enabled(term_enabled)
            .with_default_message_id(&message_id)
            .with_model(model);

        let updates = mapper.map_event(&event);
        for update in updates {
            if let Err(e) = ctx.notify_update(&sid, update) {
                tracing::warn!(session_id = %sid, error = %e, "notify_update 失败");
            }
        }
    }

    tracing::info!(?last_stop_reason, "事件流结束");

    let mut response = PromptResponse::new(last_stop_reason);

    if let Some(usage) = last_token_usage {
        response = response.usage(
            AcpUsage::new(
                usage.total_token_count as u64,
                usage.prompt_token_count as u64,
                usage.candidates_token_count as u64,
            )
            .thought_tokens(usage.thinking_token_count.map(|v| v as u64))
            .cached_read_tokens(usage.cache_read_input_token_count.map(|v| v as u64))
            .cached_write_tokens(usage.cache_creation_input_token_count.map(|v| v as u64)),
        );
    }

    if let Err(e) = responder.respond(response) {
        tracing::error!(error = %e, "respond 失败");
    }
}

// ── rewind 回显 ────────────────────────────────────────────────────

/// rewind 时发送废弃提示 + 用户消息回显。
fn send_echo_notifications(
    ctx: &dyn Context,
    sid: &str,
    blocks: &[AcpContentBlock],
    message_id: &str,
) -> Result<(), agent_client_protocol::Error> {
    ctx.notify_info(sid, "上一条已废弃")?;

    for block in blocks {
        ctx.notify_update(
            sid,
            SessionUpdate::UserMessageChunk(
                ContentChunk::new(block.clone()).message_id(message_id),
            ),
        )?;
    }

    Ok(())
}

pub(crate) async fn handle_cancel_notification(
    ctx: &dyn Context,
    notif: CancelNotification,
) -> Result<(), agent_client_protocol::Error> {
    let sid = notif.session_id.to_string();

    let runner = ctx.get_runner(&sid).await;
    if let Some(runner) = runner {
        runner.cancel().await;
    }

    Ok(())
}
