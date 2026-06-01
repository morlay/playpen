use std::str::FromStr;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use playpen_sandbox::Command;
use std::path::PathBuf;
use std::process;

use playpen_config::AppConfig;

mod commands;
mod db;
mod log;
mod printer;

use log::init_logging;

#[derive(Parser)]
#[command(name = "playpen")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Run {
        script: String,
    },
    LsAccess {
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    DomainAccess {
        #[arg(value_name = "DOMAIN")]
        domains: Vec<String>,
    },
    Config,
    Agent {
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        thinking_level: Option<String>,
        #[arg(short = 'i', long)]
        interactive: bool,
        prompt: Option<String>,
        #[command(subcommand)]
        action: Option<AgentAction>,
    },
    Acp,
    #[command(external_subcommand)]
    CatchAll(Vec<String>),
}

#[derive(Subcommand)]
enum AgentAction {
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
}

#[derive(Subcommand)]
enum SessionAction {
    Get {
        id: String,
    },
    List {
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    init_logging();

    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let app = AppConfig::load_or_default(&cwd);

    match cli.command {
        None => commands::run::interactive_mode(&cwd, &app),
        Some(Commands::Run { script }) => commands::run::run_command(&script, &cwd, &app),
        Some(Commands::CatchAll(args)) => {
            if args.is_empty() {
                commands::run::interactive_mode(&cwd, &app);
            } else {
                let cmd = Command::from_args(&args).unwrap_or_else(|e| {
                    eprintln!("playpen: {}", e);
                    process::exit(1);
                });
                commands::run::run_command(&cmd.command, &cwd, &app);
            }
        }
        Some(Commands::LsAccess { paths }) => commands::access::ls_access(&app, &cwd, &paths),
        Some(Commands::DomainAccess { domains }) => commands::access::domain_access(&app, &domains),
        Some(Commands::Config) => commands::config::print_config(&app),
        Some(Commands::Agent {
            interactive,
            prompt,
            model,
            profile,
            thinking_level,
            action,
        }) => match action {
            Some(AgentAction::Session { action }) => {
                let dirs = playpen_config::Dirs::with_defaults(&cwd);

                let db_path = crate::db::sessions_db_path();
                let db_url = format!("sqlite://{}", db_path.display());
                let pool = sqlx::SqlitePool::connect_with(
                    sqlx::sqlite::SqliteConnectOptions::from_str(&db_url)
                        .expect("无效的 session 数据库 URL")
                        .create_if_missing(true),
                )
                .await
                .expect("连接 session 数据库失败");
                let session_service: Arc<dyn playpen_session::SessionService> =
                    Arc::new(playpen_session::DBSessionService::new(pool.into()));

                let profile_resolver: Arc<dyn playpen_profile::AgentProfileLoader> =
                    Arc::new(playpen_profile::LocalAgentProfileLoader);

                let builder = playpen_agent::SimpleRunnerBuilder::new(
                    &app.settings,
                    &dirs,
                    session_service,
                    profile_resolver,
                );
                match action {
                    SessionAction::Get { id } => commands::session::get(&builder, &id).await,
                    SessionAction::List { limit, offset } => {
                        commands::session::list(&builder, limit, offset).await
                    }
                }
            }
            None => {
                commands::agent::run(
                    &cwd,
                    interactive,
                    prompt.as_deref(),
                    model.as_deref(),
                    profile.as_deref(),
                    thinking_level.as_deref(),
                    &app.settings,
                )
                .await;
            }
        },
        Some(Commands::Acp) => {
            commands::acp::run(&cwd, &app.settings).await;
        }
    }
}
