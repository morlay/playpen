use std::sync::Arc;

use agent_client_protocol::Agent;

use crate::acp_state::AcpState;
use crate::dispatch::handle_dispatch;

pub async fn serve(
    builder: Box<dyn playpen_agent::AgentRunnerBuilder>,
    transport: impl agent_client_protocol::ConnectTo<agent_client_protocol::Agent> + 'static,
) -> anyhow::Result<()> {
    let state = Arc::new(AcpState::new(builder));

    Agent
        .builder()
        .name("playpen-acp")
        .on_receive_dispatch(
            async |msg: agent_client_protocol::Dispatch, cx| handle_dispatch(msg, cx, &state).await,
            |f: &mut _, msg: agent_client_protocol::Dispatch, cx| Box::pin(f(msg, cx)),
        )
        .connect_to(transport)
        .await?;

    Ok(())
}
