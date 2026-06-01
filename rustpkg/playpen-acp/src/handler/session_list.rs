use agent_client_protocol::Responder;
use agent_client_protocol::schema::v1::{ListSessionsRequest, ListSessionsResponse, SessionInfo};

use crate::acp_state::Context;

pub(crate) async fn handle_list_sessions(
    ctx: &dyn Context,
    _req: ListSessionsRequest,
    responder: Responder<ListSessionsResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let svc = ctx.builder().sessions();
    let sessions = svc.list(None, 0).await.map_err(|e| {
        agent_client_protocol::util::internal_error(format!("list sessions 失败: {e}"))
    })?;

    let mut result = Vec::with_capacity(sessions.len());
    for s in &sessions {
        let id = s.id().to_string();
        let cwd = s
            .state()
            .get("cwd")
            .await
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        let title = s
            .state()
            .get("title")
            .await
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| id.clone());
        result.push(
            SessionInfo::new(agent_client_protocol::schema::v1::SessionId::from(id), cwd)
                .title(title),
        );
    }

    responder.respond(ListSessionsResponse::new(result))?;
    Ok(())
}
