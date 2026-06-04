//! Session 管理 handler：NewSession / Load / List / Resume / Close / SetConfigOption。
//!
//! 基于新的接口体系：
//! - `SessionManager`（内存）做 CRUD
//! - `FileBackedSessionStore` 做文件持久化
//! - `Registry` 管理模型注册
//! - `ProfileManager` 管理 profile 与技能

use agent_client_protocol::schema::{
    AvailableCommand, AvailableCommandsUpdate,
    CloseSessionRequest, CloseSessionResponse,
    ContentBlock, ContentChunk, ListSessionsRequest, ListSessionsResponse,
    LoadSessionRequest, LoadSessionResponse, NewSessionRequest, NewSessionResponse,
    ResumeSessionRequest, ResumeSessionResponse, SessionConfigKind, SessionConfigOption,
    SessionConfigSelect, SessionConfigSelectOption, SessionInfo, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, SetSessionConfigOptionResponse,
    TextContent,
};
use agent_client_protocol::{ConnectionTo, Responder};
use agent_client_protocol::role::acp::Client;
use playpen_agent_core::model::Model;
use playpen_agent_core::profile::Profile;

use crate::agent::AcpState;

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 从 NewSessionRequest 的 `meta._playpen` 中提取 agent_name 和 model_id。
///
/// 格式：`{ agent_name?, model_id? }`
fn extract_playpen_meta(
    meta: &Option<serde_json::Map<String, serde_json::Value>>,
) -> (String, String) {
    const DEFAULT_AGENT: &str = "default";
    const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-pro";

    let Some(map) = meta else {
        return (DEFAULT_AGENT.into(), DEFAULT_MODEL.into());
    };
    let Some(playpen) = map.get("_playpen") else {
        return (DEFAULT_AGENT.into(), DEFAULT_MODEL.into());
    };

    let agent_name = playpen
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_AGENT)
        .to_string();
    let model_id = playpen
        .get("model_id")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_MODEL)
        .to_string();
    (agent_name, model_id)
}

/// 根据 `{provider}/{model_id}` 格式解析 Model。
///
/// 若没有 `/` 分隔，则尝试在所有 provider 中查找。
fn resolve_model(
    state: &AcpState,
    model_key: &str,
) -> Result<Model, agent_client_protocol::Error> {
    let err_not_found = || {
        agent_client_protocol::util::internal_error(format!("未找到模型: {model_key}"))
    };

    if let Some((provider, model_id)) = model_key.split_once('/') {
        state
            .registry
            .find_model(provider, model_id)
            .ok_or_else(err_not_found)
    } else {
        // 无 provider 前缀，遍历所有模型
        state
            .registry
            .list_models()
            .into_iter()
            .find(|m| m.id == model_key)
            .ok_or_else(err_not_found)
    }
}

/// 构建 Session Config Options（mode / model / thought_level）。
fn build_config_options(
    state: &AcpState,
    current_agent: &str,
    current_model_key: &str,
    current_thought_level: String,
) -> Vec<SessionConfigOption> {
    let mut options: Vec<SessionConfigOption> = Vec::new();

    // ---- mode: profile 选择 ----
    let mode_select = if state.profiles.is_empty() {
        SessionConfigSelect::new(
            "default",
            vec![SessionConfigSelectOption::new("default", "默认")],
        )
    } else {
        let current = if state.profiles.iter().any(|p| p.name == current_agent) {
            current_agent.to_string()
        } else {
            state.profiles[0].name.clone()
        };
        let profile_opts: Vec<SessionConfigSelectOption> = state
            .profiles
            .iter()
            .map(|p| {
                SessionConfigSelectOption::new(p.name.clone(), p.name.clone())
                    .description(p.description.as_deref().unwrap_or(""))
            })
            .collect();
        SessionConfigSelect::new(current, profile_opts)
    };
    options.push(
        SessionConfigOption::new("mode", "会话模式", SessionConfigKind::Select(mode_select))
            .description("选择 Agent 配置（含环境信息与技能）")
            .category(agent_client_protocol::schema::SessionConfigOptionCategory::Mode),
    );

    // ---- model: 模型选择 ----
    let all_models = state.registry.list_models_with_provider();
    let model_opts: Vec<SessionConfigSelectOption> = all_models
        .iter()
        .map(|(provider, m)| {
            let key = format!("{}/{}", provider, m.id);
            let label = format!("{}: {}", provider, m.name);
            SessionConfigSelectOption::new(key, label)
        })
        .collect();

    // 确认当前值是否有效
    let default_model_key = if all_models.iter().any(|(p, m)| {
        format!("{}/{}", p, m.id) == current_model_key
    }) {
        current_model_key.to_string()
    } else if let Some((p, m)) = all_models.first() {
        format!("{}/{}", p, m.id)
    } else {
        current_model_key.to_string()
    };

    options.push(
        SessionConfigOption::new("model", "模型", SessionConfigKind::Select(
            SessionConfigSelect::new(default_model_key, model_opts),
        ))
        .category(agent_client_protocol::schema::SessionConfigOptionCategory::Model),
    );

    // ---- thought_level: 思考等级（仅在模型支持推理时） ----
    let current_model = all_models.iter().find(|(p, m)| {
        format!("{}/{}", p, m.id) == current_model_key
    });
    if let Some((_, model)) = current_model
        && !model.reasoning_efforts.is_empty() {
            let all_levels = ["off".to_string(),
                "low".to_string(),
                "medium".to_string(),
                "high".to_string()];
            let level_opts: Vec<SessionConfigSelectOption> = all_levels
                .iter()
                .map(|l| SessionConfigSelectOption::new(l.clone(), l.clone()))
                .collect();
            let selected = if all_levels.contains(&current_thought_level) {
                current_thought_level.clone()
            } else {
                "off".to_string()
            };
            options.push(
                SessionConfigOption::new(
                    "thought_level",
                    "思考等级",
                    SessionConfigKind::Select(SessionConfigSelect::new(selected, level_opts)),
                )
                .category(
                    agent_client_protocol::schema::SessionConfigOptionCategory::ThoughtLevel,
                ),
            );
        }

    options
}

/// 构建可用命令清单（slash commands = skills）。
fn build_available_commands(state: &AcpState, profile: &Profile) -> Vec<AvailableCommand> {
    let mut commands: Vec<AvailableCommand> = Vec::new();

    // skills 作为 slash commands（格式: skill:{name}）
    let skill_enabled = profile.skill_enabled.unwrap_or(true);
    if skill_enabled {
        for skill in state.profile_manager.list_skills() {
            let name = format!("skill:{}", skill.name);
            commands.push(AvailableCommand::new(name, &skill.description));
        }
    }

    commands
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// 处理 `session/new` 请求。
pub async fn handle_new_session(
    req: NewSessionRequest,
    responder: Responder<NewSessionResponse>,
    cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    let cwd = req.cwd.to_string_lossy().to_string();
    let (agent_name, model_id) = extract_playpen_meta(&req.meta);

    tracing::info!(
        cwd = %cwd,
        agent_name = %agent_name,
        model_id = %model_id,
        "收到 new_session 请求"
    );

    let model = resolve_model(state, &model_id)?;

    // 构建 Env 与 system_prompt
    let env = state
        .profile_manager
        .build_env(&agent_name)
        .map_err(|e| {
            agent_client_protocol::util::internal_error(format!("构建环境失败: {e}"))
        })?;
    let system_prompt = env.build_system_prompt();

    let title = format!("{} - {}", agent_name, &cwd[..cwd.len().min(40)]);
    let session_id = state.session_store.create(
        &title,
        model,
        &cwd,
        &agent_name,
        &system_prompt,
        Vec::new(), // tools_schema 由运行时按需加载
        None,       // context_window 使用模型默认值
    );

    tracing::info!(session_id = %session_id, "session 创建成功");

    // 构建 config options
    let config_options = build_config_options(state, &agent_name, &model_id, "off".to_string());

    // 发送可用命令清单（slash commands）
    let profile = find_profile(state, &agent_name);
    let commands = build_available_commands(state, &profile);
    let cmd_notif = SessionNotification::new(
        session_id.clone(),
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
    );
    let _ = cx.send_notification(cmd_notif);

    let response = NewSessionResponse::new(
        agent_client_protocol::schema::SessionId::from(session_id),
    )
    .config_options(config_options);

    responder.respond(response)?;
    Ok(())
}

/// 处理 `session/load` 请求：加载 session 并回放历史消息。
pub async fn handle_load_session(
    req: LoadSessionRequest,
    responder: Responder<LoadSessionResponse>,
    cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    let sid_str = req.session_id.to_string();
    tracing::info!(session_id = %sid_str, "收到 load_session 请求");

    let session = state
        .session_manager
        .get(&sid_str)
        .ok_or_else(|| {
            agent_client_protocol::util::internal_error(format!("session 未找到: {sid_str}"))
        })?;

    // 流式回放历史消息
    for msg in &session.messages {
        if let Some(text) = msg.text_content() {
            let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(text.to_string())));
            let notif = SessionNotification::new(
                req.session_id.clone(),
                SessionUpdate::AgentMessageChunk(chunk),
            );
            cx.send_notification(notif).map_err(|e| {
                agent_client_protocol::util::internal_error(format!("发送回放通知失败: {e}"))
            })?;
        }
    }

    let response = LoadSessionResponse::default();
    responder.respond(response)?;

    // 发送 config options
    let thought_level = state
        .session_thought_levels
        .lock()
        .unwrap()
        .get(&sid_str)
        .cloned()
        .unwrap_or_else(|| "off".to_string());
    let provider = resolve_provider_for_model(state, &session.model.id)
        .unwrap_or_else(|| "unknown".to_string());
    let model_key = format!("{}/{}", provider, session.model.id);
    let config_options = build_config_options(state, &session.agent_name, &model_key, thought_level);
    let config_notif = SessionNotification::new(
        req.session_id.clone(),
        SessionUpdate::ConfigOptionUpdate(
            agent_client_protocol::schema::ConfigOptionUpdate::new(config_options),
        ),
    );
    let _ = cx.send_notification(config_notif);

    // 发送 slash commands
    let profile = find_profile(state, &session.agent_name);
    let commands = build_available_commands(state, &profile);
    let cmd_notif = SessionNotification::new(
        req.session_id.clone(),
        SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(commands)),
    );
    let _ = cx.send_notification(cmd_notif);

    Ok(())
}

/// 处理 `session/list` 请求。
pub async fn handle_list_sessions(
    _req: ListSessionsRequest,
    responder: Responder<ListSessionsResponse>,
    _cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    tracing::info!("收到 list_sessions 请求");

    let sessions: Vec<SessionInfo> = state
        .session_manager
        .list()
        .into_iter()
        .map(|s| {
            SessionInfo::new(
                agent_client_protocol::schema::SessionId::from(s.id),
                s.project_root.to_string_lossy().to_string(),
            )
            .title(s.title)
        })
        .collect();

    responder.respond(ListSessionsResponse::new(sessions))?;
    Ok(())
}

/// 处理 `session/resume` 请求（不重放消息）。
pub async fn handle_resume_session(
    req: ResumeSessionRequest,
    responder: Responder<ResumeSessionResponse>,
    _cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    let sid_str = req.session_id.to_string();
    tracing::info!(session_id = %sid_str, "收到 resume_session 请求");

    state.session_manager.get(&sid_str).ok_or_else(|| {
        agent_client_protocol::util::internal_error(format!("session 未找到: {sid_str}"))
    })?;

    responder.respond(ResumeSessionResponse::default())?;
    Ok(())
}

/// 处理 `session/close` 请求（归档 session）。
pub async fn handle_close_session(
    req: CloseSessionRequest,
    responder: Responder<CloseSessionResponse>,
    _cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    let sid_str = req.session_id.to_string();
    tracing::info!(session_id = %sid_str, "收到 close_session 请求");

    state.session_manager.archive(&sid_str).map_err(|e| {
        tracing::error!(error = %e, session_id = %sid_str, "归档 session 失败");
        agent_client_protocol::util::internal_error(format!("归档 session 失败: {e}"))
    })?;

    // 归档后立即持久化
    state.session_store.persist(&sid_str);

    responder.respond(CloseSessionResponse::default())?;
    Ok(())
}

/// 处理 `session/set_config_option` 请求。
pub async fn handle_set_config_option(
    req: SetSessionConfigOptionRequest,
    responder: Responder<SetSessionConfigOptionResponse>,
    _cx: ConnectionTo<Client>,
    state: &AcpState,
) -> Result<(), agent_client_protocol::Error> {
    let sid_str = req.session_id.to_string();
    tracing::info!(
        session_id = %sid_str,
        config_id = %req.config_id,
        value = %req.value,
        "收到 set_config_option 请求"
    );

    match req.config_id.to_string().as_str() {
        "model" => {
            let model = resolve_model(state, &req.value.0)?;
            state.session_manager.update_model(&sid_str, model).map_err(|e| {
                agent_client_protocol::util::internal_error(format!("更新模型失败: {e}"))
            })?;
            state.session_store.persist(&sid_str);
        }
        "thought_level" => {
            state.session_thought_levels.lock().unwrap().insert(
                sid_str.clone(),
                req.value.0.to_string(),
            );
            tracing::info!(
                session_id = %sid_str,
                thought_level = %req.value.0,
                "thinking level 已变更"
            );
        }
        _ => {
            tracing::debug!(config_id = %req.config_id, "未处理的 config option");
        }
    }

    // 返回当前完整配置
    let session = state.session_manager.get(&sid_str).ok_or_else(|| {
        agent_client_protocol::util::internal_error(format!("session 未找到: {sid_str}"))
    })?;
    let provider = resolve_provider_for_model(state, &session.model.id)
        .unwrap_or_else(|| "unknown".to_string());
    let model_key = format!("{}/{}", provider, session.model.id);
    let thought_level = state
        .session_thought_levels
        .lock()
        .unwrap()
        .get(&sid_str)
        .cloned()
        .unwrap_or_else(|| "off".to_string());
    let config_options = build_config_options(state, &session.agent_name, &model_key, thought_level);

    responder.respond(SetSessionConfigOptionResponse::new(config_options))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

/// 按名称查找 profile，找不到则返回空 profile。
fn find_profile(state: &AcpState, agent_name: &str) -> Profile {
    state
        .profiles
        .iter()
        .find(|p| p.name == agent_name)
        .cloned()
        .unwrap_or_else(|| Profile {
            name: agent_name.to_string(),
            description: None,
            active_tools: None,
            skill_enabled: None,
            system_prompt: String::new(),
        })
}

/// 根据 model_id 查找所属 provider 名称。
pub(super) fn resolve_provider_for_model(state: &AcpState, model_id: &str) -> Option<String> {
    state
        .registry
        .list_models_with_provider()
        .into_iter()
        .find(|(_, m)| m.id == model_id)
        .map(|(p, _)| p)
}
