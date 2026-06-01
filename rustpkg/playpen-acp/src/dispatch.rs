use std::sync::Arc;

use agent_client_protocol::role::acp::Client;
use agent_client_protocol::schema::v1::{
    CancelNotification, CloseSessionRequest, DeleteSessionRequest, InitializeRequest,
    ListSessionsRequest, LoadSessionRequest, NewSessionRequest, PromptRequest,
    ResumeSessionRequest, SetSessionConfigOptionRequest,
};
use agent_client_protocol::util::MatchDispatch;
use agent_client_protocol::{ConnectionTo, Dispatch, util};
use tracing::info_span;

use crate::acp_state::{AcpState, AcpStateContext};
use crate::handler::{initialize, prompt_turn, session_delete, session_list, session_setup};

macro_rules! handle_request {
    ($d:expr, $req_ty:ty, $handler:path, $state:expr, $cx:expr) => {
        $d.if_request::<$req_ty, _>({
            let ctx = AcpStateContext::new($state, $cx);
            async move |req, responder| $handler(&ctx, req, responder).await
        })
        .await
    };
}

macro_rules! handle_notification {
    ($d:expr, $notif_ty:ty, $handler:path, $state:expr, $cx:expr) => {
        $d.if_notification::<$notif_ty, _>({
            let ctx = AcpStateContext::new($state, $cx);
            async move |notif| $handler(&ctx, notif).await
        })
        .await
    };
}

/// `handle_prompt` 需要 owned context 给 spawn。
macro_rules! handle_prompt {
    ($d:expr, $req_ty:ty, $handler:path, $state:expr, $cx:expr) => {
        $d.if_request::<$req_ty, _>({
            let ctx = AcpStateContext::new($state, $cx);
            async move |req, responder| $handler(ctx, req, responder).await
        })
        .await
    };
}

/// 从 untyped Dispatch 中提取 method 名称
fn extract_method(msg: &Dispatch) -> &str {
    match msg {
        Dispatch::Request(req, _) => req.method(),
        Dispatch::Notification(notif) => notif.method(),
        Dispatch::Response(_, router) => router.method(),
    }
}

/// 从 untyped Dispatch 中提取 params（JSON Value）
fn extract_params(msg: &Dispatch) -> Option<&serde_json::Value> {
    match msg {
        Dispatch::Request(req, _) => Some(req.params()),
        Dispatch::Notification(notif) => Some(notif.params()),
        Dispatch::Response(_, _) => None,
    }
}

/// 从 untyped Dispatch 的 params 中提取 session_id
fn extract_session_id(msg: &Dispatch) -> Option<String> {
    extract_params(msg).and_then(|params| params.get("session_id")?.as_str().map(|s| s.to_string()))
}

pub async fn handle_dispatch(
    msg: Dispatch,
    cx: ConnectionTo<Client>,
    state: &Arc<AcpState>,
) -> Result<(), agent_client_protocol::Error> {
    let method = extract_method(&msg).to_string();
    let sid = extract_session_id(&msg).unwrap_or_default();
    let params = extract_params(&msg);

    let params_value = params.cloned().unwrap_or(serde_json::Value::Null);

    // span 的 enter/exit 自动覆盖 inbound/outbound 的生命周期
    // JSON 模式下 FmtSpan::FULL 会记录 new/enter/exit/close 事件
    // 每个事件都自动携带 method / session_id / request 字段
    let span = info_span!("acp.receive",
        method = %method,
        session_id = %sid,
        request = %params_value,
    );
    let _guard = span.enter();

    let result = dispatch_inner(msg, cx, state).await;

    if let Err(e) = &result {
        tracing::warn!(error = %e, "acp.receive 失败");
    }

    result
}

async fn dispatch_inner(
    msg: Dispatch,
    cx: ConnectionTo<Client>,
    state: &Arc<AcpState>,
) -> Result<(), agent_client_protocol::Error> {
    let mut d = MatchDispatch::new(msg);

    d = handle_request!(
        d,
        InitializeRequest,
        initialize::handle_initialize,
        state,
        &cx
    );
    d = handle_request!(
        d,
        NewSessionRequest,
        session_setup::handle_new_session,
        state,
        &cx
    );
    d = handle_request!(
        d,
        LoadSessionRequest,
        session_setup::handle_load_session,
        state,
        &cx
    );
    d = handle_request!(
        d,
        ListSessionsRequest,
        session_list::handle_list_sessions,
        state,
        &cx
    );
    d = handle_request!(
        d,
        ResumeSessionRequest,
        session_setup::handle_resume_session,
        state,
        &cx
    );
    d = handle_request!(
        d,
        CloseSessionRequest,
        session_setup::handle_close_session,
        state,
        &cx
    );
    d = handle_request!(
        d,
        DeleteSessionRequest,
        session_delete::handle_delete_session,
        state,
        &cx
    );
    d = handle_request!(
        d,
        SetSessionConfigOptionRequest,
        session_setup::handle_set_config_option,
        state,
        &cx
    );
    d = handle_prompt!(d, PromptRequest, prompt_turn::handle_prompt, state, &cx);
    d = handle_notification!(
        d,
        CancelNotification,
        prompt_turn::handle_cancel_notification,
        state,
        &cx
    );
    // --- fallback: 未匹配的消息 ---
    d.otherwise(async |msg| {
        let method = extract_method(&msg).to_string();
        tracing::warn!(method, "未处理的 ACP 消息");
        if let Dispatch::Request(_, responder) = msg {
            responder.respond_with_error(util::internal_error(format!("未知方法: {method}")))?;
        }
        Ok(())
    })
    .await
}
