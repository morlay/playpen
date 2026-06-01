use clap::{Parser, Subcommand};
use playpen_zed_agent::{agent, zed};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use sandbox::Sandbox;
use sandbox::config::{self, ValidationResult};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use std::process;

/// playpen——沙盒命令执行器。
#[derive(Parser)]
#[command(name = "playpen")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 执行 shell 脚本
    Run { script: String },
    /// 查询路径的文件系统访问规则（支持 glob）
    LsAccess {
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// 查询域名的网络访问规则
    DomainAccess {
        #[arg(value_name = "DOMAIN")]
        domains: Vec<String>,
    },
    /// 生成 zed agent 配置
    Setup {
        #[command(subcommand)]
        target: SetupTarget,
    },
    /// 捕获未知命令，作为 run 执行
    #[command(external_subcommand)]
    CatchAll(Vec<String>),
}

#[derive(Subcommand)]
enum SetupTarget {
    /// 生成并写入 zed agent 的 tool_permissions
    ZedAgent {
        profile: String,
        #[arg(long)]
        write: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let toml_config = load_config(&cwd);

    match cli.command {
        None => interactive_mode(&cwd, &toml_config),
        Some(Commands::Run { script }) => {
            run_command(&script, &cwd, &toml_config);
        }
        Some(Commands::CatchAll(args)) => {
            if args.is_empty() {
                interactive_mode(&cwd, &toml_config);
            } else {
                run_command(&args.join(" "), &cwd, &toml_config);
            }
        }
        Some(Commands::LsAccess { paths }) => ls_access(&toml_config, &cwd, &paths),
        Some(Commands::DomainAccess { domains }) => domain_access(&toml_config, &domains),
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

                let sandbox_config = toml_config.to_sandbox_config();
                if let Err(e) = zed::setup_zed_agent(
                    &sandbox_config,
                    &cwd,
                    &profile,
                    &global_settings,
                    project_settings,
                    write,
                ) {
                    eprintln!("playpen: 写入 zed 配置失败: {}", e);
                    process::exit(1);
                }

                // 生成 AGENTS.md
                if let Some(zed_agent) = load_zed_agent_config() {
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

fn run_command(command: &str, cwd: &std::path::Path, playpen_config: &PlaypenConfig) {
    let shell = shell_bin();
    let sandbox_config = Sandbox::create_config(&playpen_config.to_sandbox_config(), cwd, &shell);
    match Sandbox::exec(command, cwd, &sandbox_config) {
        Ok(output) => process::exit(output.code),
        Err(e) => {
            eprintln!("playpen: {}", e);
            process::exit(1);
        }
    }
}

fn ls_access(playpen_config: &PlaypenConfig, cwd: &std::path::Path, paths: &[String]) {
    let rules = playpen_config
        .filesystem
        .as_ref()
        .and_then(|f| f.access.as_deref())
        .map(config::parse_filesystem_string)
        .unwrap_or_default();

    for path in paths {
        let clean = path.trim_start_matches("./");
        let target = cwd.join(clean);
        let rule = config::find_filesystem_rule(&rules, cwd, &target);
        match rule {
            Some(r) => println!("{} {}", rule_label(&r.prefix), target.display()),
            None => println!("-- {}", target.display()),
        }
    }
}

fn domain_access(playpen_config: &PlaypenConfig, domains: &[String]) {
    let rules = playpen_config
        .network
        .as_ref()
        .and_then(|n| n.access.as_deref())
        .map(config::parse_network_string)
        .unwrap_or_default();

    for domain in domains {
        let result = config::validate_network_domain(&rules, domain);
        match result {
            ValidationResult::Allowed => println!("ALLOW {}", domain),
            ValidationResult::Denied => println!("DENY  {}", domain),
            _ => {}
        }
    }
}

fn rule_label(prefix: &config::RulePrefix) -> &'static str {
    match prefix {
        config::RulePrefix::Allow => "rw",
        config::RulePrefix::ReadOnly => "r-",
        config::RulePrefix::Deny => "--",
    }
}

fn shell_bin() -> String {
    std::env::var("SHELL")
        .unwrap_or_else(|_| "/bin/bash".into())
        .trim()
        .to_string()
}

fn dirs_fallback() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

// ---- 配置加载 ----

fn load_config(cwd: &std::path::Path) -> PlaypenConfig {
    let global_path = std::env::var("PLAYPEN_GLOBAL_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_fallback().join(".config/playpen.toml"));
    let project_path = cwd.join(".playpen.toml");

    let global_config = read_playpen_toml(&global_path);
    let project_config = read_playpen_toml(&project_path);

    PlaypenConfig::merge(global_config, project_config)
}

fn read_playpen_toml(path: &std::path::Path) -> Option<PlaypenConfig> {
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(path) {
        Ok(raw) => match toml::from_str(&raw) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("警告：TOML 解析错误 {}：{}", path.display(), e);
                None
            }
        },
        Err(e) => {
            eprintln!("警告：无法读取 {}：{}", path.display(), e);
            None
        }
    }
}

// ---- agents 配置（仅全局） ----

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PlaypenConfig {
    #[serde(default)]
    network: Option<config::AllowSection>,
    #[serde(default)]
    filesystem: Option<config::AllowSection>,
    #[serde(default)]
    shell: Option<config::ShellSection>,
    #[serde(default)]
    agents: Option<AgentSections>,
}

impl PlaypenConfig {
    fn merge(global: Option<PlaypenConfig>, project: Option<PlaypenConfig>) -> PlaypenConfig {
        match (global, project) {
            (None, None) => PlaypenConfig::default(),
            (None, Some(p)) => p,
            (Some(g), None) => g,
            (Some(g), Some(p)) => Self {
                network: merge_allow_section(
                    g.network.as_ref().and_then(|n| n.access.as_deref()),
                    p.network.as_ref().and_then(|n| n.access.as_deref()),
                ),
                filesystem: merge_allow_section(
                    g.filesystem.as_ref().and_then(|f| f.access.as_deref()),
                    p.filesystem.as_ref().and_then(|f| f.access.as_deref()),
                ),
                shell: merge_shell_section(g.shell, p.shell),
                agents: p.agents.or(g.agents),
            },
        }
    }

    fn to_sandbox_config(&self) -> config::Config {
        config::Config {
            network: self.network.clone(),
            filesystem: self.filesystem.clone(),
            shell: self.shell.clone(),
        }
    }
}

fn merge_allow_section(
    global: Option<&str>,
    project: Option<&str>,
) -> Option<config::AllowSection> {
    match (global, project) {
        (None, None) => None,
        (Some(g), None) => Some(config::AllowSection {
            access: Some(g.to_string()),
        }),
        (None, Some(p)) => Some(config::AllowSection {
            access: Some(p.to_string()),
        }),
        (Some(g), Some(p)) => Some(config::AllowSection {
            access: Some(format!("{}\n{}", g, p)),
        }),
    }
}

fn merge_shell_section(
    global: Option<config::ShellSection>,
    project: Option<config::ShellSection>,
) -> Option<config::ShellSection> {
    match (global, project) {
        (None, None) => None,
        (None, Some(p)) => Some(p),
        (Some(g), None) => Some(g),
        (Some(g), Some(p)) => {
            let allow =
                merge_allow_section(g.allow.as_deref(), p.allow.as_deref()).and_then(|a| a.access);
            Some(config::ShellSection {
                allow_pipe: p.allow_pipe.or(g.allow_pipe),
                allow_multiple: p.allow_multiple.or(g.allow_multiple),
                allow,
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentSections {
    zed: Option<agent::ZedAgentConfig>,
}

/// 进入交互模式：使用 readline 逐行读取输入，每条命令通过沙盒执行。
fn interactive_mode(cwd: &std::path::Path, toml_config: &PlaypenConfig) {
    let shell = shell_bin();
    let sandbox_config = Sandbox::create_config(&toml_config.to_sandbox_config(), cwd, &shell);

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
                    Err(e) => {
                        eprintln!("playpen: {}", e);
                    }
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

fn load_zed_agent_config() -> Option<agent::ZedAgentConfig> {
    let home = dirs_fallback();
    let path = home.join(".config/playpen.toml");
    if !path.exists() {
        return None;
    }
    let raw = fs::read_to_string(&path).ok()?;
    let config: PlaypenConfig = toml::from_str(&raw).ok()?;
    config.agents.and_then(|a| a.zed)
}
