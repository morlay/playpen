use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use futures::StreamExt;
use playpen_agent::{AgentRunnerBuilder, SimpleRunnerBuilder};
use playpen_config::Settings;
use playpen_content::ContentBlock;
use playpen_profile::AgentProfileLoader;
use playpen_session::DBSessionService;

use crate::printer::Printer;

pub async fn run(
    cwd: &Path,
    interactive: bool,
    prompt: Option<&str>,
    model: Option<&str>,
    profile_name: Option<&str>,
    thinking_level: Option<&str>,
    settings: &Settings,
) {
    if let Err(e) = run_async(
        cwd,
        interactive,
        prompt,
        model,
        profile_name,
        thinking_level,
        settings,
    )
    .await
    {
        eprintln!("agent 错误: {e}");
    }
}

async fn run_async(
    cwd: &Path,
    interactive: bool,
    prompt: Option<&str>,
    model: Option<&str>,
    profile_name: Option<&str>,
    thinking_level: Option<&str>,
    settings: &Settings,
) -> anyhow::Result<()> {
    crate::log::init_logging();

    let dirs = playpen_config::Dirs::with_defaults(cwd);

    let db_path = crate::db::sessions_db_path();
    let db_url = format!("sqlite://{}", db_path.display());
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::from_str(&db_url)?.create_if_missing(true),
    )
    .await?;
    let session_svc = DBSessionService::new(pool.into());
    session_svc.migrate().await?;
    let session_service: Arc<dyn playpen_session::SessionService> = Arc::new(session_svc);

    let profile_resolver: Arc<dyn AgentProfileLoader> =
        Arc::new(playpen_profile::LocalAgentProfileLoader);

    let builder = SimpleRunnerBuilder::new(settings, &dirs, session_service, profile_resolver);

    let profile = load_profile(profile_name, model, thinking_level)?;

    if interactive {
        run_interactive(&builder, prompt, profile).await?;
    } else {
        run_once(&builder, prompt.unwrap_or(""), profile).await?;
    }
    Ok(())
}

fn load_profile(
    name: Option<&str>,
    model: Option<&str>,
    thinking_level: Option<&str>,
) -> anyhow::Result<Box<dyn playpen_profile::AgentProfile>> {
    let dirs = playpen_config::Dirs::with_defaults(&std::env::current_dir()?);
    let resolver = playpen_profile::LocalAgentProfileLoader;
    let profiles = resolver.agent_profiles(&dirs)?;

    let profile = if let Some(n) = name {
        profiles
            .into_iter()
            .find(|p| p.name() == n)
            .ok_or_else(|| anyhow::anyhow!("profile '{n}' 未找到"))?
    } else {
        profiles
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("没有可用的 AgentProfile"))?
    };

    if model.is_some() || thinking_level.is_some() {
        Ok(profile.with_model_profile(&|mp| {
            let mut mp = mp.clone();
            if let Some(m) = model {
                mp.model = m.to_string();
            }
            if let Some(tl) = thinking_level {
                mp.thinking_level = Some(match tl {
                    "off" => playpen_config::model::ThinkingLevel::Off,
                    "high" => playpen_config::model::ThinkingLevel::High,
                    "max" => playpen_config::model::ThinkingLevel::Max,
                    _ => playpen_config::model::ThinkingLevel::Off,
                });
            }
            mp
        }))
    } else {
        Ok(profile)
    }
}

async fn run_once(
    builder: &dyn AgentRunnerBuilder,
    input: &str,
    profile: Box<dyn playpen_profile::AgentProfile>,
) -> anyhow::Result<()> {
    let mut printer = Printer::new();
    let runner = builder.create(profile).await?;
    let mut stream = runner.run(vec![ContentBlock::from(input)]).await;
    while let Some(event) = stream.next().await {
        printer.print(&event);
    }
    Ok(())
}

async fn run_interactive(
    builder: &dyn AgentRunnerBuilder,
    first: Option<&str>,
    profile: Box<dyn playpen_profile::AgentProfile>,
) -> anyhow::Result<()> {
    use rustyline::DefaultEditor;
    use rustyline::error::ReadlineError;

    let runner = builder.create(profile).await?;

    let mut rl = DefaultEditor::new()?;
    let mut input = first.map(|s| s.to_string()).unwrap_or_default();
    if input.is_empty() {
        eprintln!("(输入 exit 退出)");
    }

    loop {
        if input.is_empty() {
            match rl.readline("> ") {
                Ok(line) => {
                    let t = line.trim();
                    if t.is_empty() {
                        continue;
                    }
                    if t == "exit" || t == "quit" {
                        break;
                    }
                    let _ = rl.add_history_entry(&line);
                    input = t.to_string();
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
                Err(e) => {
                    eprintln!("{e}");
                    break;
                }
            }
        }

        let mut printer = Printer::new();
        let mut stream = runner.run(vec![ContentBlock::from(input.as_str())]).await;
        while let Some(ae) = stream.next().await {
            printer.print(&ae);
        }
        input.clear();
    }
    Ok(())
}
