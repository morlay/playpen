use agent_client_protocol::schema::v1::{
    CloseSessionRequest, CloseSessionResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, ResumeSessionRequest, ResumeSessionResponse,
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelect,
    SessionConfigSelectOption, SessionConfigValueId, SessionId, SetSessionConfigOptionRequest,
    SetSessionConfigOptionResponse,
};
use agent_client_protocol::{Responder, util};

use crate::acp_state::{Context, PendingConfig, send_available_commands};

// ── helpers ──────────────────────────────────────────────────────────

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(c).collect(),
    }
}

fn build_config_options(
    ctx: &dyn Context,
    settings: &playpen_config::Settings,
    current_profile: &str,
    current_model_key: &str,
    current_thought_level: &str,
) -> Vec<SessionConfigOption> {
    let mut options = Vec::new();

    let profiles = ctx.builder().agent_profiles().unwrap_or_default();
    let mode_opts: Vec<_> = profiles
        .iter()
        .map(|p| SessionConfigSelectOption::new(p.name().to_string(), capitalize(p.name())))
        .collect();

    let current_mode = if mode_opts
        .iter()
        .any(|o| o.value.to_string() == current_profile)
    {
        SessionConfigValueId::from(current_profile.to_string())
    } else {
        mode_opts
            .first()
            .map(|o| o.value.clone())
            .unwrap_or_else(|| SessionConfigValueId::from("default"))
    };
    if !mode_opts.is_empty() {
        options.push(
            SessionConfigOption::new(
                "mode",
                "模式",
                SessionConfigKind::Select(SessionConfigSelect::new(current_mode, mode_opts)),
            )
            .category(SessionConfigOptionCategory::Mode),
        );
    }

    let model_opts: Vec<_> = settings
        .model_providers
        .iter()
        .flat_map(|(provider, p)| {
            p.models.as_ref().into_iter().flat_map(move |models| {
                models.iter().map(move |m| {
                    let key = format!("{provider}/{}", m.name);
                    let label = m
                        .display_name
                        .clone()
                        .unwrap_or_else(|| format!("{}: {}", provider, m.name));
                    SessionConfigSelectOption::new(key, label)
                })
            })
        })
        .collect();

    let current_key = if model_opts
        .iter()
        .any(|o| o.value.to_string() == current_model_key)
    {
        current_model_key.to_string()
    } else {
        model_opts
            .first()
            .map(|o| o.value.to_string())
            .unwrap_or_default()
    };
    if !model_opts.is_empty() {
        options.push(
            SessionConfigOption::new(
                "model",
                "模型",
                SessionConfigKind::Select(SessionConfigSelect::new(
                    current_key.clone(),
                    model_opts,
                )),
            )
            .category(SessionConfigOptionCategory::Model),
        );
    }

    let level_opts = {
        let model = settings.find_model(&current_key);
        let efforts = model.map(|m| &m.reasoning_efforts);
        let mut opts = vec![SessionConfigSelectOption::new("off", "关闭")];
        if efforts.is_none_or(|e| e.contains(&playpen_config::model::ThinkingLevel::High)) {
            opts.push(SessionConfigSelectOption::new("high", "高"));
        }
        if efforts.is_none_or(|e| e.contains(&playpen_config::model::ThinkingLevel::Max)) {
            opts.push(SessionConfigSelectOption::new("max", "最大"));
        }
        opts
    };
    let selected = if level_opts
        .iter()
        .any(|o| o.value.to_string() == current_thought_level)
    {
        current_thought_level.to_string()
    } else {
        "off".to_string()
    };
    options.push(
        SessionConfigOption::new(
            "thought_level",
            "思考等级",
            SessionConfigKind::Select(SessionConfigSelect::new(selected, level_opts)),
        )
        .category(SessionConfigOptionCategory::ThoughtLevel),
    );

    options
}

async fn replay_session_events_inner(
    ctx: &dyn Context,
    sid: &str,
    project_root: &std::path::Path,
    term_enabled: bool,
) -> Result<(), agent_client_protocol::Error> {
    use crate::event_mapper::EventMapper;
    use futures::StreamExt;

    let runner = ctx
        .builder()
        .resume(sid)
        .await
        .map_err(|e| util::internal_error(format!("replay 恢复 session 失败: {e}")))?;

    let message_id = "replay";
    let model = runner
        .settings()
        .find_model(&runner.profile().model_profile().model);
    let mut stream = runner.replay();
    let mut replay_count = 0usize;
    while let Some(event) = stream.next().await {
        let mapper = EventMapper::new(project_root)
            .with_term_enabled(term_enabled)
            .with_replay(true)
            .with_default_message_id(message_id)
            .with_model(model);
        let updates = mapper.map_event(&event);
        for update in updates {
            ctx.notify_update(sid, update)?;
        }
        replay_count += 1;
    }
    tracing::info!(replay_count, "replay 完成");

    Ok(())
}

pub(crate) async fn replay_session_events(
    ctx: &dyn Context,
    sid: &str,
    project_root: &std::path::Path,
    term_enabled: bool,
) -> Result<(), agent_client_protocol::Error> {
    replay_session_events_inner(ctx, sid, project_root, term_enabled).await
}

// ── helpers for merging pending config ───────────────────────────────

/// 只有 pending 中存在的字段才覆盖，其余保持原值。
/// 返回应用了配置变更后的新 runner。
pub(crate) fn apply_pending_config(
    pending: &PendingConfig,
    runner: Box<dyn playpen_agent::AgentRunner>,
) -> Box<dyn playpen_agent::AgentRunner> {
    let profile = runner.profile();
    let model = pending
        .model_key
        .as_deref()
        .unwrap_or_else(|| profile.model_profile().model.as_str());

    let thought = pending
        .thought_level
        .clone()
        .or_else(|| {
            profile
                .model_profile()
                .thinking_level
                .as_ref()
                .map(|tl| format!("{:?}", tl).to_lowercase())
        })
        .unwrap_or_else(|| "off".to_string());

    let updated = profile.with_model_profile(&|mp| {
        let mut mp = mp.clone();
        mp.model = model.to_string();
        if let Ok(tl) = serde_json::from_value::<playpen_config::model::ThinkingLevel>(
            serde_json::Value::String(thought.to_string()),
        ) {
            mp.thinking_level = Some(tl);
        }
        mp
    });

    runner.with_profile(updated)
}

/// 获取用于 config options 展示的 (name, model, thought)，合并 pending
struct SessionConfig {
    name: String,
    model_key: String,
    thought_level: String,
    settings: playpen_config::Settings,
}

async fn current_config(
    ctx: &dyn Context,
    sid: &str,
    pending: Option<&PendingConfig>,
) -> Result<SessionConfig, agent_client_protocol::Error> {
    let runner = ctx
        .builder()
        .resume(sid)
        .await
        .map_err(|e| util::internal_error(format!("session {sid} 未找到: {e}")))?;
    let p = runner.profile();

    let name = pending
        .and_then(|c| c.profile_name.clone())
        .unwrap_or_else(|| p.name().to_string());
    let model_key = pending
        .and_then(|c| c.model_key.clone())
        .unwrap_or_else(|| p.model_profile().model.clone());
    let thought_level = pending
        .and_then(|c| c.thought_level.clone())
        .or_else(|| {
            p.model_profile()
                .thinking_level
                .as_ref()
                .map(|tl| format!("{:?}", tl).to_lowercase())
        })
        .unwrap_or_else(|| "off".to_string());

    Ok(SessionConfig {
        name,
        model_key,
        thought_level,
        settings: runner.settings().clone(),
    })
}

// ── handlers ─────────────────────────────────────────────────────────

pub(crate) async fn handle_new_session(
    ctx: &dyn Context,
    _req: NewSessionRequest,
    responder: Responder<NewSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let default_profile: Box<dyn playpen_profile::AgentProfile> = ctx
        .builder()
        .agent_profiles()
        .unwrap_or_default()
        .into_iter()
        .next()
        .ok_or_else(|| util::internal_error("没有可用的 AgentProfile"))?;

    let runner = ctx
        .builder()
        .create(default_profile)
        .await
        .map_err(|e| util::internal_error(format!("创建 session 失败: {e}")))?;

    let sid = runner.id().to_string();
    let skills = runner.profile().available_skills().unwrap_or_default();

    let pending = ctx.get_pending_config(&sid);

    let cfg = current_config(ctx, &sid, pending.as_ref()).await?;
    let opts = build_config_options(
        ctx,
        &cfg.settings,
        &cfg.name,
        &cfg.model_key,
        &cfg.thought_level,
    );

    responder
        .respond(NewSessionResponse::new(SessionId::from(sid.clone())).config_options(opts))?;

    // ugly delay for new only
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    send_available_commands(ctx, &sid, &skills);

    Ok(())
}

pub(crate) async fn handle_load_session(
    ctx: &dyn Context,
    req: LoadSessionRequest,
    responder: Responder<LoadSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let sid = req.session_id.to_string();

    let pending = ctx.get_pending_config(&sid);

    let runner = ctx
        .builder()
        .resume(&sid)
        .await
        .map_err(|e| util::internal_error(format!("session {sid} 未找到: {e}")))?;
    let project_root = runner.profile().working_dir().clone();
    let term_enabled = ctx.has_flag("terminal_output");
    let skills = runner.profile().available_skills().unwrap_or_default();

    // 应用 pending 配置到 runner（runner 在后续 replay 等操作中由内部重新获取）
    if let Some(ref c) = pending {
        apply_pending_config(c, runner);
    }

    // replay 在 respond 之前
    if let Err(e) = replay_session_events(ctx, &sid, &project_root, term_enabled).await {
        tracing::error!(error = %e, "replay 失败");
    }

    let cfg = current_config(ctx, &sid, pending.as_ref()).await?;
    let opts = build_config_options(
        ctx,
        &cfg.settings,
        &cfg.name,
        &cfg.model_key,
        &cfg.thought_level,
    );

    responder.respond(LoadSessionResponse::new().config_options(opts))?;

    send_available_commands(ctx, &sid, &skills);

    Ok(())
}

pub(crate) async fn handle_resume_session(
    ctx: &dyn Context,
    req: ResumeSessionRequest,
    responder: Responder<ResumeSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let sid = req.session_id.to_string();

    let pending = ctx.get_pending_config(&sid);

    if let Ok(r) = ctx.builder().resume(&sid).await
        && let Some(ref c) = pending
    {
        let runner = apply_pending_config(c, r);

        let config_result = current_config(ctx, &sid, pending.as_ref()).await.ok();
        if let Some(cfg) = config_result {
            let opts = build_config_options(
                ctx,
                &cfg.settings,
                &cfg.name,
                &cfg.model_key,
                &cfg.thought_level,
            );
            responder.respond(ResumeSessionResponse::default().config_options(opts))?;
            if let Ok(skills) = runner.profile().available_skills() {
                send_available_commands(ctx, &sid, &skills);
            }
        }
    }

    Ok(())
}

pub(crate) async fn handle_close_session(
    _ctx: &dyn Context,
    req: CloseSessionRequest,
    responder: Responder<CloseSessionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let _sid = req.session_id.to_string();
    responder.respond(CloseSessionResponse::default())?;
    Ok(())
}

pub(crate) async fn handle_set_config_option(
    ctx: &dyn Context,
    req: SetSessionConfigOptionRequest,
    responder: Responder<SetSessionConfigOptionResponse>,
) -> Result<(), agent_client_protocol::Error> {
    let sid = req.session_id.to_string();
    let config_id = req.config_id.to_string();
    let value = req
        .value
        .as_value_id()
        .map(|v| v.to_string())
        .unwrap_or_default();

    ctx.put_pending_config(&sid, &config_id, &value);

    let pending = ctx.get_pending_config(&sid);
    let cfg = current_config(ctx, &sid, pending.as_ref()).await?;
    let opts = build_config_options(
        ctx,
        &cfg.settings,
        &cfg.name,
        &cfg.model_key,
        &cfg.thought_level,
    );
    responder.respond(SetSessionConfigOptionResponse::new(opts))?;

    Ok(())
}
