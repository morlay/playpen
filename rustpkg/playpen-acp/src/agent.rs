//! AcpState 持有者与启动入口。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use playpen_agent_core::agent::settings::Settings;
use playpen_agent_core::config::AppConfig;
use playpen_agent_core::model::Registry;
use playpen_agent_core::profile::manager::ProfileManager;
use playpen_agent_core::profile::Profile;
use playpen_agent_core::session::store::SessionManager;
use playpen_agent_core::workspace::Workspace;
use playpen_agent_core::workspace::{create_sandbox_config, filesystem_rules};

use agent_client_protocol::schema::{
    CancelNotification, CloseSessionRequest, InitializeRequest, ListSessionsRequest,
    LoadSessionRequest, NewSessionRequest, PromptRequest, ResumeSessionRequest,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol::{on_receive_notification, on_receive_request, Agent, ConnectTo, Stdio};

use crate::handler::{initialize, prompt, session};
use crate::session_store::FileBackedSessionStore;

/// ACP Agent 全局共享状态。
pub struct AcpState {
    pub session_manager: Arc<SessionManager>,
    pub session_store: Arc<FileBackedSessionStore>,
    pub registry: Arc<Registry>,
    pub profile_manager: Arc<ProfileManager>,
    pub workspace: Arc<Workspace>,
    pub settings: Arc<Settings>,
    pub profiles: Vec<Profile>,
    /// session_id → cancel flag
    pub cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// session_id → thought_level
    pub session_thought_levels: Arc<Mutex<HashMap<String, String>>>,
    /// 测试用：直接注入事件流（测试端持有 sender，AcpState 持有 receiver）
    pub mock_event_rx: tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<playpen_agent_core::agent::runner::AgentEvent>>>,
}

/// 构建 AcpState。
pub fn build_state(cwd: &PathBuf) -> anyhow::Result<Arc<AcpState>> {
    let config = AppConfig::load_or_default(cwd);

    // 如果 conf.d 未配置 providers，使用内置
    let providers = if config.providers.is_empty() {
        tracing::info!("使用内置 providers");
        playpen_agent_core::model::builtin_providers()
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect()
    } else {
        config.providers
    };

    let settings = Arc::new(config.settings);
    let registry = Arc::new(Registry::new(providers));

    let profile_manager = Arc::new(ProfileManager::new(
        playpen_agent_core::profile::Loader::new(cwd.clone()),
    ));
    let profiles = profile_manager.list_profiles().unwrap_or_default();

    let session_manager = Arc::new(SessionManager::new());
    let session_store = {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        Arc::new(FileBackedSessionStore::new(
            home.join(".config").join("playpen").join("sessions"),
            session_manager.clone(),
        )?)
    };

    let full_sandbox = create_sandbox_config(&config.sandbox, cwd);
    let rules = filesystem_rules(&config.sandbox);
    let workspace = Arc::new(Workspace::new(cwd.clone(), Arc::new(full_sandbox), rules));

    Ok(Arc::new(AcpState {
        session_manager,
        session_store,
        registry,
        profile_manager,
        workspace,
        settings,
        profiles,
        cancel_flags: Arc::new(Mutex::new(HashMap::new())),
        session_thought_levels: Arc::new(Mutex::new(HashMap::new())),
        mock_event_rx: tokio::sync::Mutex::new(None),
    }))
}

/// 启动 ACP Agent，通过 Stdio 与 Client 通信。
pub async fn run(state: Arc<AcpState>) -> anyhow::Result<()> {
    run_with_transport(state, Stdio::new()).await
}

/// 启动 ACP Agent，通过自定义 Transport 与 Client 通信。
pub async fn run_with_transport(
    state: Arc<AcpState>,
    transport: impl ConnectTo<Agent> + 'static,
) -> anyhow::Result<()> {
    let s_init = state.clone();
    let s_session = state.clone();
    let s_prompt = state.clone();
    let s_cancel = state.clone();

    Agent.builder()
        .name("playpen-acp")
        .on_receive_request(
            {
                let s = s_init.clone();
                async move |req: InitializeRequest, responder, cx| {
                    initialize::handle_initialize(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: NewSessionRequest, responder, cx| {
                    session::handle_new_session(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: LoadSessionRequest, responder, cx| {
                    session::handle_load_session(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: ListSessionsRequest, responder, cx| {
                    session::handle_list_sessions(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: ResumeSessionRequest, responder, cx| {
                    session::handle_resume_session(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: CloseSessionRequest, responder, cx| {
                    session::handle_close_session(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_prompt.clone();
                async move |req: PromptRequest, responder, cx| {
                    prompt::handle_prompt(req, responder, cx, s.clone()).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_request(
            {
                let s = s_session.clone();
                async move |req: SetSessionConfigOptionRequest, responder, cx| {
                    session::handle_set_config_option(req, responder, cx, &s).await
                }
            },
            on_receive_request!(),
        )
        .on_receive_notification(
            {
                let s = s_cancel.clone();
                async move |notif: CancelNotification, cx| {
                    prompt::handle_cancel_notification(notif, cx, s.clone()).await
                }
            },
            on_receive_notification!(),
        )
        .connect_to(transport)
        .await?;

    Ok(())
}
