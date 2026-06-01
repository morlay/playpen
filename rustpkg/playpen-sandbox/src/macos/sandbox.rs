use std::path::{Path, PathBuf};

use crate::config::{
    self,
    policy::ShellPolicy,
    shell::{ShellConfig, check_shell},
};
use crate::macos::seatbelt;
use crate::sandbox::{AccessVerdict, Command, Error, Sandbox, Verdict};

pub struct MacosSandbox {
    working_dir: PathBuf,
    exec_policy: ShellPolicy,
    allow_pipe: bool,
    allow_multiple: bool,
    filesystem_rules: Vec<config::ParsedRule>,
}

impl MacosSandbox {
    pub fn new(toml_config: &config::Config, cwd: &Path) -> Self {
        let filesystem_rules = toml_config
            .filesystem
            .as_ref()
            .map(|f| config::parse_filesystem_access(&f.access))
            .unwrap_or_default();

        let shell_section = toml_config.shell.clone().unwrap_or_default();
        let exec_policy = ShellPolicy::from_rules(&shell_section.allow);

        Self {
            working_dir: cwd.to_path_buf(),
            exec_policy,
            allow_pipe: shell_section.allow_pipe.unwrap_or(true),
            allow_multiple: shell_section.allow_multiple.unwrap_or(false),
            filesystem_rules,
        }
    }
}

impl Sandbox for MacosSandbox {
    fn access(&self, uri: &str) -> AccessVerdict {
        if let Some(path) = uri.strip_prefix("file://") {
            let target = Path::new(path);
            let resolved = if target.is_relative() {
                self.working_dir.join(target)
            } else {
                target.to_path_buf()
            };
            let verdict = match config::validate_filesystem_path(
                &self.filesystem_rules,
                &self.working_dir,
                &resolved,
            ) {
                config::ValidationResult::Allowed => Verdict::Allowed,
                config::ValidationResult::ReadOnly => Verdict::ReadOnly,
                config::ValidationResult::Denied => Verdict::Denied,
            };
            AccessVerdict::new(verdict, uri)
        } else {
            AccessVerdict::denied(uri)
        }
    }

    fn wrap_command(&self, mut cmd: Command) -> Result<Command, Error> {
        let shell_config = ShellConfig {
            allow_pipe: self.allow_pipe,
            allow_multiple: self.allow_multiple,
        };

        let check = check_shell(&cmd.command, &shell_config, &self.exec_policy);
        if !check.allowed {
            let reason = check.reason.unwrap_or_else(|| "命令不允许执行".to_string());
            return Err(Error::Forbidden(format!("Shell 规则拒绝 — {reason}")));
        }

        if cmd.cwd.is_none() {
            cmd.cwd = Some(self.working_dir.clone());
        }

        // 生成 seatbelt profile 并用 sandbox-exec 包装命令
        if !self.filesystem_rules.is_empty() {
            let policy = config::PolicyClassification::from_parsed_rules(
                &self.filesystem_rules,
                &self.working_dir,
            );
            let profile = seatbelt::generate_profile(&policy);

            let profile_path = self.write_profile(&profile)?;
            let profile_path_str = profile_path.to_string_lossy().into_owned();
            let quoted_cmd = shlex::try_quote(&cmd.command)
                .map_err(|e| Error::Unexpected(format!("转义命令失败: {e}")))?;
            let quoted_path = shlex::try_quote(&profile_path_str)
                .map_err(|e| Error::Unexpected(format!("转义路径失败: {e}")))?;
            cmd.command = format!("sandbox-exec -f {quoted_path} sh -c {quoted_cmd}");
        }

        Ok(cmd)
    }
}

impl MacosSandbox {
    fn write_profile(&self, profile: &str) -> Result<PathBuf, Error> {
        let tmp_dir = std::env::temp_dir();
        let suffix: String = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        let path = tmp_dir.join(format!("playpen_{suffix}.sbpl"));
        std::fs::write(&path, profile)
            .map_err(|e| Error::Unexpected(format!("写入 seatbelt profile 失败: {e}")))?;
        Ok(path)
    }
}

#[cfg(test)]
#[path = "sandbox_test.rs"]
mod tests;
