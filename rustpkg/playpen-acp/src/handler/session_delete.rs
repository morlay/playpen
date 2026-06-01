use agent_client_protocol::Responder;
use agent_client_protocol::schema::v1::{DeleteSessionRequest, DeleteSessionResponse};

use crate::acp_state::Context;

pub(crate) async fn handle_delete_session(
    ctx: &dyn Context,
    req: DeleteSessionRequest,
    responder: Responder<DeleteSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let sid = req.session_id.to_string();
    ctx.builder().sessions().delete(&sid).await.map_err(|e| {
        agent_client_protocol::util::internal_error(format!("删除 session 失败: {e}"))
    })?;
    responder.respond(DeleteSessionResponse::default())?;
    Ok(())
}
