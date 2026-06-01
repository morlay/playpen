use agent_client_protocol::Responder;
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ClientCapabilities, Implementation, InitializeRequest, InitializeResponse,
    PromptCapabilities, SessionCapabilities, SessionCloseCapabilities, SessionDeleteCapabilities,
    SessionListCapabilities, SessionResumeCapabilities,
};

use crate::acp_state::Context;

fn resolve_terminal_output(caps: &ClientCapabilities) -> bool {
    caps.meta
        .as_ref()
        .and_then(|m| m.get("terminal_output"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn build_agent_capabilities() -> AgentCapabilities {
    AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(
            PromptCapabilities::new()
                .image(false)
                .audio(false)
                .embedded_context(true),
        )
        .session_capabilities(
            SessionCapabilities::new()
                .list(SessionListCapabilities::new())
                .resume(SessionResumeCapabilities::new())
                .close(SessionCloseCapabilities::new())
                .delete(SessionDeleteCapabilities::new()),
        )
}

fn build_initialize_response(req: &InitializeRequest) -> InitializeResponse {
    InitializeResponse::new(req.protocol_version)
        .agent_capabilities(build_agent_capabilities())
        .agent_info(
            Implementation::new("playpen-acp", env!("CARGO_PKG_VERSION"))
                .title("Playpen ACP Agent"),
        )
}

pub(crate) async fn handle_initialize(
    ctx: &dyn Context,
    req: InitializeRequest,
    responder: Responder<InitializeResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let terminal_output = resolve_terminal_output(&req.client_capabilities);
    ctx.set_flag("terminal_output", terminal_output);

    let response = build_initialize_response(&req);
    responder.respond(response)?;
    Ok(())
}
