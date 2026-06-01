use std::path::Path;
use std::process::Command;

use crate::policy::{ShellPolicy, classify_policy};
use crate::shell::{ShellConfig, check_shell};

pub struct Sandbox;

impl Sandbox {
    pub fn create_config(
        toml_config: &crate::config::Config,
        cwd: &Path,
        shell_bin: &str,
    ) -> SandboxConfig {
        let filesystem_rules = toml_config
            .filesystem
            .as_ref()
            .and_then(|f| f.access.as_deref())
            .map(crate::config::parse_filesystem_string)
            .unwrap_or_default();

        let policy_class = classify_policy(&filesystem_rules, cwd);

        let shell_section = toml_config.shell.clone().unwrap_or_default();

        let exec_policy = shell_section
            .allow
            .as_deref()
            .map(ShellPolicy::from_raw)
            .unwrap_or_default();

        SandboxConfig {
            shell_bin: shell_bin.to_string(),
            policy_class,
            exec_policy,
            allow_pipe: shell_section.allow_pipe.unwrap_or(true),
            allow_multiple: shell_section.allow_multiple.unwrap_or(false),
        }
    }

    pub fn exec(command: &str, cwd: &Path, config: &SandboxConfig) -> Result<ExecOutput, String> {
        let shell_config = ShellConfig {
            allow_pipe: config.allow_pipe,
            allow_multiple: config.allow_multiple,
        };

        let check = check_shell(command, &shell_config, &config.exec_policy);
        if !check.allowed {
            let reason = check.reason.unwrap_or_else(|| "命令不允许执行".to_string());
            return Err(format!("playpen: Shell 规则拒绝执行 — {}", reason));
        }

        let profile = crate::seatbelt::generate_profile(&config.policy_class);

        if std::env::var("PLAYPEN_DEBUG").is_ok() {
            eprintln!("// === seatbelt profile ===\n{}", profile);
        }

        let rc = if config.shell_bin.contains("zsh") {
            "source ~/.zshrc 2>/dev/null; "
        } else if config.shell_bin.contains("bash") {
            "source ~/.bash_profile 2>/dev/null; "
        } else {
            ""
        };
        let wrapped = format!("{}{}", rc, command);

        let status = spawn_sandboxed(&profile, &config.shell_bin, cwd, &["-l", "-c", &wrapped])?;
        exit_code(status)
    }

    /// 启动沙盒化的交互式 shell（跳过命令规则检查，直接透传给用户）。
    pub fn exec_interactive(cwd: &Path, config: &SandboxConfig) -> Result<ExecOutput, String> {
        let profile = crate::seatbelt::generate_profile(&config.policy_class);

        if std::env::var("PLAYPEN_DEBUG").is_ok() {
            eprintln!("// === seatbelt profile ===\n{}", profile);
        }

        // 交互式 shell：-i 进入交互模式，-l 加载 login profile
        let status = spawn_sandboxed(&profile, &config.shell_bin, cwd, &["-i", "-l"])?;
        exit_code(status)
    }
}

#[derive(Debug)]
pub struct ExecOutput {
    pub code: i32,
}

pub struct SandboxConfig {
    pub shell_bin: String,
    pub policy_class: crate::policy::PolicyClassification,
    pub exec_policy: ShellPolicy,
    pub allow_pipe: bool,
    pub allow_multiple: bool,
}

fn spawn_sandboxed(
    profile: &str,
    shell_bin: &str,
    cwd: &Path,
    args: &[&str],
) -> Result<std::process::ExitStatus, String> {
    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-p")
        .arg(profile)
        .arg(shell_bin)
        .current_dir(cwd)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .env("CLICOLOR_FORCE", "1")
        .env("TMPDIR", "/tmp")
        .env("PLAYPEN_SANDBOXED", "1");
    for arg in args {
        cmd.arg(arg);
    }
    cmd.spawn()
        .and_then(|mut c| c.wait())
        .map_err(|e| format!("执行失败：{}", e))
}

fn exit_code(status: std::process::ExitStatus) -> Result<ExecOutput, String> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let code = status.code().unwrap_or_else(|| {
            if let Some(sig) = status.signal() {
                128 + sig
            } else {
                -1
            }
        });
        Ok(ExecOutput { code })
    }
    #[cfg(not(unix))]
    {
        Ok(ExecOutput {
            code: status.code().unwrap_or(-1),
        })
    }
}
