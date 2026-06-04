//! InitializeRequest 处理。

use agent_client_protocol::role::acp::Client;
use agent_client_protocol::schema::{
    AgentAuthCapabilities, AgentCapabilities, Implementation, InitializeRequest,
    InitializeResponse, McpCapabilities, PromptCapabilities, SessionCapabilities,
};
use agent_client_protocol::{ConnectionTo, Responder};

use crate::agent::AcpState;

/// 处理 `initialize` 请求。
pub async fn handle_initialize(
    req: InitializeRequest,
    responder: Responder<InitializeResponse>,
    _cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    tracing::info!(
        protocol_version = ?req.protocol_version,
        profiles = state.profiles.len(),
        "收到 initialize 请求"
    );

    let capabilities = AgentCapabilities::new()
        .load_session(true)
        .prompt_capabilities(
            PromptCapabilities::new()
                .image(false)
                .audio(false)
                .embedded_context(false),
        )
        .mcp_capabilities(McpCapabilities::new().http(false).sse(false))
        .session_capabilities(
            SessionCapabilities::new()
                .list(agent_client_protocol::schema::SessionListCapabilities::new())
                .resume(agent_client_protocol::schema::SessionResumeCapabilities::new())
                .close(agent_client_protocol::schema::SessionCloseCapabilities::new()),
        )
        .auth(AgentAuthCapabilities::new());

    let models_meta = build_models_meta(state);

    let mut response_meta = serde_json::Map::new();
    response_meta.insert("_playpen".to_string(), models_meta);

    let response = InitializeResponse::new(req.protocol_version)
        .agent_capabilities(capabilities)
        .auth_methods(vec![])
        .agent_info(Implementation::new("playpen-acp", env!("CARGO_PKG_VERSION")).title("Playpen ACP Agent"))
        .meta(response_meta.clone());

    tracing::info!(
        models_count = state.registry.list_models().len(),
        "initialize 完成"
    );
    responder.respond(response)?;
    Ok(())
}

/// 构建 meta._playpen.models JSON。
fn build_models_meta(state: &AcpState) -> serde_json::Value {
    let models: Vec<serde_json::Value> = state
        .registry
        .list_models_with_provider()
        .into_iter()
        .map(|(provider, mc)| {
            serde_json::json!({
                "id": mc.id,
                "name": mc.name,
                "provider": provider,
                "context_window": mc.context_window,
                "max_tokens": mc.max_tokens,
                "reasoning": !mc.reasoning_efforts.is_empty(),
                "input": mc.input.iter().map(|t| format!("{:?}", t).to_lowercase()).collect::<Vec<_>>(),
                "cost": serde_json::json!({
                    "input": mc.cost.input,
                    "output": mc.cost.output,
                    "cache_read": mc.cost.cache_read,
                }),
            })
        })
        .collect();

    serde_json::json!({
        "models": models,
        "default_provider": state.settings.default_provider.as_deref().unwrap_or("deepseek"),
        "default_model": state.settings.default_model.as_deref().unwrap_or("deepseek-v4-pro"),
    })
}
