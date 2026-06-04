//! `playpen agent` 子命令

use std::sync::Arc;
use std::collections::HashMap;

use playpen_agent_core::agent::runner::AgentEvent;
use playpen_agent_core::agent::service::AgentSessionService;
use playpen_agent_core::config::AppConfig;
use playpen_agent_core::model::{Provider, Registry};
use playpen_agent_core::model::builtin_providers;
use playpen_agent_core::profile::Loader;
use playpen_agent_core::profile::manager::ProfileManager;
use playpen_agent_core::session::store::SessionManager;
use playpen_agent_core::workspace;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use super::printer::Printer;

pub fn run(
    cwd: &std::path::Path,
    model_key: &str,
    _thinking_level: &str,
    interactive: bool,
    prompt: Option<&str>,
) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        if let Err(e) = run_async(cwd, model_key, interactive, prompt).await {
            eprintln!("agent 错误: {}", e);
        }
    });
}

async fn build_session(cwd: &std::path::Path, model_key: &str) -> anyhow::Result<(AgentSessionService, playpen_agent_core::model::Model)> {
    let (provider_id, model_id) = model_key.split_once('/')
        .unwrap_or(("deepseek", model_key));

    let app = AppConfig::load_or_default(cwd);

    let mut providers: HashMap<String, Provider> = HashMap::new();
    for p in builtin_providers() {
        providers.insert(p.id.clone(), p);
    }
    for (name, p) in app.providers {
        providers.insert(name, p);
    }

    let registry = Registry::new(providers);
    let client = registry.build_client(provider_id)?;

    let model = registry.find_model(provider_id, model_id)
        .ok_or_else(|| anyhow::anyhow!("模型不存在: {}/{}", provider_id, model_id))?;

    let sandbox_config = workspace::create_sandbox_config(&app.sandbox, cwd);
    let filesystem_rules = workspace::filesystem_rules(&app.sandbox);
    let ws = Arc::new(workspace::Workspace::new(
        cwd.to_path_buf(),
        Arc::new(sandbox_config),
        filesystem_rules,
    ));

    let loader = Loader::new(cwd.to_path_buf());
    let profile_manager = Arc::new(ProfileManager::new(loader));
    let session_manager = Arc::new(SessionManager::new());

    let service = AgentSessionService::new(session_manager, profile_manager, client, ws, None);
    Ok((service, model))
}

async fn run_prompt(
    session: &playpen_agent_core::agent::service::AgentSession,
    input: &str,
) -> anyhow::Result<()> {
    let mut rx = session.prompt(input);
    let mut printer = Printer::new();

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::TextDelta(t) => printer.push(&t),
            AgentEvent::ToolCallStart { name, arguments, .. } => {
                printer.emit(&format!("🔧 {} {}", name, arguments));
            }
            AgentEvent::ToolCallResult { id, result } => {
                printer.emit(&format!("📤 {}: {}", id, if result.len() > 200 { &result[..200] } else { &result }));
            }
            AgentEvent::Done { usage, .. } => {
                printer.commit();
                eprintln!("tokens: {} in / {} out", usage.input, usage.output);
                return Ok(());
            }
            AgentEvent::Error(e) => {
                printer.commit();
                eprintln!("错误: {}", e);
                return Ok(());
            }
            _ => {}
        }
    }
    Ok(())
}

async fn run_async(cwd: &std::path::Path, model_key: &str, interactive: bool, prompt: Option<&str>) -> anyhow::Result<()> {
    let (service, model) = build_session(cwd, model_key).await?;
    let session = service.new_session(model).await?;
    eprintln!("session: {}", session.session_id);

    if interactive {
        let mut rl = DefaultEditor::new()?;
        loop {
            match rl.readline("> ") {
                Ok(line) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() { continue; }
                    if trimmed == "exit" || trimmed == "quit" { break; }
                    let _ = rl.add_history_entry(&line);
                    run_prompt(&session, trimmed).await?;
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
                Err(e) => {
                    eprintln!("读取失败: {}", e);
                    break;
                }
            }
        }
    } else {
        let input = prompt.unwrap_or("你好").to_string();
        run_prompt(&session, &input).await?;
    }

    Ok(())
}
