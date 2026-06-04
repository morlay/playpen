use clap::{Parser, Subcommand};
use playpen_zed_agent::{agent, zed};

mod commands;
mod config;

use playpen_agent_core::config::AppConfig;
use playpen_agent_core::workspace as core_sandbox;
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use sandbox::Sandbox;
use sandbox::config::RulePrefix;
use std::path::PathBuf;
use std::process;

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
    Setup {
        #[command(subcommand)]
        target: SetupTarget,
    },
    Agent {
        #[arg(long, default_value = "deepseek/deepseek-v4-pro")]
        model: String,
        #[arg(long, default_value = "off")]
        thinking_level: String,
        #[arg(short = 'i', long)]
        interactive: bool,
        prompt: Option<String>,
    },
    Acp,
    #[command(external_subcommand)]
    CatchAll(Vec<String>),
}

#[derive(Subcommand)]
enum SetupTarget {
    ZedAgent {
        profile: String,
        #[arg(long)]
        write: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let app = AppConfig::load_or_default(&cwd);

    match cli.command {
        None => interactive_mode(&cwd, &app),
        Some(Commands::Run { script }) => run_command(&script, &cwd, &app),
        Some(Commands::CatchAll(args)) => {
            if args.is_empty() {
                interactive_mode(&cwd, &app);
            } else {
                let cmd = sandbox::shell::join_args(&args).unwrap_or_else(|e| {
                    eprintln!("playpen: {}", e);
                    process::exit(1);
                });
                run_command(&cmd, &cwd, &app);
            }
        }
        Some(Commands::LsAccess { paths }) => ls_access(&app, &cwd, &paths),
        Some(Commands::DomainAccess { domains }) => domain_access(&app, &domains),
        Some(Commands::Config) => print_config(&app),
        Some(Commands::Agent {
            model,
            thinking_level,
            interactive,
            prompt,
        }) => {
            commands::agent::run(
                &cwd,
                &model,
                &thinking_level,
                interactive,
                prompt.as_deref(),
            );
        }
        Some(Commands::Acp) => {
            commands::acp::run(&cwd);
        }
        Some(Commands::Setup { target }) => match target {
            SetupTarget::ZedAgent { profile, write } => {
                let home = dirs_fallback();
                let global_settings = home.join(".config/zed/settings.json");
                let project_settings_path = cwd.join(".zed/settings.json");
                let project_settings =
                    if project_settings_path.exists() || cwd.join(".playpen.toml").exists() {
                        Some(project_settings_path.as_path())
                    } else {
                        None
                    };
                if let Err(e) = zed::setup_zed_agent(
                    &core_sandbox::filesystem_rules(&app.sandbox),
                    &cwd,
                    &profile,
                    &global_settings,
                    project_settings,
                    write,
                ) {
                    eprintln!("playpen: 写入 zed 配置失败: {}", e);
                    process::exit(1);
                }
                if let Some(zed_agent) = config::load_zed_agent_config() {
                    let agents_md_path = home.join(".config/zed/AGENTS.md");
                    if let Err(e) = agent::generate_agents_md(&zed_agent, &agents_md_path, write) {
                        eprintln!("playpen: AGENTS.md 失败: {}", e);
                        process::exit(1);
                    }
                }
            }
        },
    }
}

fn run_command(command: &str, cwd: &std::path::Path, app: &AppConfig) {
    let s = core_sandbox::create_sandbox_config(&app.sandbox, cwd);
    match Sandbox::exec(command, cwd, &s) {
        Ok(output) => process::exit(output.code),
        Err(e) => {
            eprintln!("playpen: {}", e);
            process::exit(1);
        }
    }
}

fn ls_access(app: &AppConfig, cwd: &std::path::Path, paths: &[String]) {
    let rules = core_sandbox::filesystem_rules(&app.sandbox);
    for path in paths {
        let clean = path.trim_start_matches("./");
        let target = cwd.join(clean);
        let rule = sandbox::config::find_filesystem_rule(&rules, cwd, &target);
        match rule {
            Some(r) => println!("{} {}", rule_label(&r.prefix), target.display()),
            None => println!("-- {}", target.display()),
        }
    }
}

fn domain_access(app: &AppConfig, domains: &[String]) {
    let rules = core_sandbox::network_rules(&app.sandbox);
    for domain in domains {
        let result = sandbox::config::validate_network_domain(&rules, domain);
        match result {
            sandbox::config::ValidationResult::Allowed => println!("ALLOW {}", domain),
            sandbox::config::ValidationResult::Denied => println!("DENY  {}", domain),
            _ => {}
        }
    }
}

fn rule_label(prefix: &RulePrefix) -> &'static str {
    match prefix {
        RulePrefix::Allow => "rw",
        RulePrefix::ReadOnly => "r-",
        RulePrefix::Deny => "--",
    }
}

fn print_config(app: &AppConfig) {
    let mut output = String::new();

    if let Ok(s) = toml::to_string_pretty(&app) {
        output.push_str(&s);
    }

    println!("{}", output.trim_end());
}

fn dirs_fallback() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

fn interactive_mode(cwd: &std::path::Path, app: &AppConfig) {
    let sandbox_config = core_sandbox::create_sandbox_config(&app.sandbox, cwd);
    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("playpen: 初始化 readline 失败: {}", e);
            process::exit(1);
        }
    };
    loop {
        match rl.readline("$ ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }
                let _ = rl.add_history_entry(&line);
                match Sandbox::exec(trimmed, cwd, &sandbox_config) {
                    Ok(output) => {
                        if output.code != 0 {
                            eprintln!("playpen: 命令退出码 {}", output.code);
                        }
                    }
                    Err(e) => eprintln!("playpen: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("playpen: 读取失败: {}", err);
                break;
            }
        }
    }
}
