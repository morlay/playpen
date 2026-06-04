use std::path::{Path, PathBuf};
use std::sync::Arc;

use sandbox::config::ParsedRule;
pub use sandbox::config::Config as SandboxConfig;
pub use sandbox::config::ValidationResult;
pub use sandbox::sandbox::ExecOutput;
pub use sandbox::SandboxConfig as FullSandboxConfig;

/// 从 Config 创建 SandboxConfig
pub fn create_sandbox_config(config: &SandboxConfig, cwd: &Path) -> FullSandboxConfig {
    let shell_bin = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    sandbox::Sandbox::create_config(config, cwd, &shell_bin)
}

/// 从 Config 提取 filesystem 规则
pub fn filesystem_rules(config: &SandboxConfig) -> Vec<ParsedRule> {
    config
        .filesystem
        .as_ref()
        .map(|f| sandbox::config::parse_filesystem_access(&f.access))
        .unwrap_or_default()
}

/// 从 Config 提取 network 规则
pub fn network_rules(config: &SandboxConfig) -> Vec<ParsedRule> {
    config
        .network
        .as_ref()
        .map(|n| sandbox::config::parse_network_access(&n.access))
        .unwrap_or_default()
}

pub fn validate_path(rules: &[ParsedRule], cwd: &Path, target: &Path) -> ValidationResult {
    sandbox::config::validate_filesystem_path(rules, cwd, target)
}

pub fn validate_network_domain(rules: &[ParsedRule], domain: &str) -> ValidationResult {
    sandbox::config::validate_network_domain(rules, domain)
}

pub fn exec_in_sandbox(
    command: &str,
    cwd: &Path,
    config: &FullSandboxConfig,
) -> anyhow::Result<sandbox::sandbox::ExecOutput> {
    sandbox::Sandbox::exec(command, cwd, config)
        .map_err(|e| anyhow::anyhow!("沙箱执行失败：{}", e))
}

// ── Workspace ──

pub struct Workspace {
    pub project_root: PathBuf,
    pub sandbox_config: Arc<FullSandboxConfig>,
    pub filesystem_rules: Vec<ParsedRule>,
}

impl Workspace {
    pub fn new(
        project_root: PathBuf,
        sandbox_config: Arc<FullSandboxConfig>,
        filesystem_rules: Vec<ParsedRule>,
    ) -> Self {
        Self { project_root, sandbox_config, filesystem_rules }
    }

    /// 相对路径自动补全为 project_root 下的绝对路径
    pub fn resolve_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_relative() {
            self.project_root.join(p)
        } else {
            p.to_path_buf()
        }
    }

    pub fn check_path(&self, target: &Path) -> ValidationResult {
        validate_path(&self.filesystem_rules, &self.project_root, target)
    }

    /// 读取文件（沙箱校验）
    pub fn read_file(&self, path: &Path) -> Result<String, WorkspaceError> {
        if matches!(self.check_path(path), ValidationResult::Denied) {
            return Err(WorkspaceError::SandboxDenied(path.display().to_string()));
        }
        std::fs::read_to_string(path).map_err(|e| WorkspaceError::Io {
            path: path.display().to_string(),
            source: e,
        })
    }

    /// 写入文件（须 Allow 权限）
    pub fn write_file(&self, path: &Path, content: &str) -> Result<(), WorkspaceError> {
        match self.check_path(path) {
            ValidationResult::Allowed => {}
            ValidationResult::ReadOnly => {
                return Err(WorkspaceError::SandboxDenied(format!("只读: {}", path.display())));
            }
            ValidationResult::Denied => {
                return Err(WorkspaceError::SandboxDenied(path.display().to_string()));
            }
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceError::Io {
                path: parent.display().to_string(), source: e,
            })?;
        }
        std::fs::write(path, content).map_err(|e| WorkspaceError::Io {
            path: path.display().to_string(),
            source: e,
        })
    }

    pub fn walk_files(
        &self,
        search_path: &Path,
        glob_filter: Option<&str>,
    ) -> anyhow::Result<Vec<PathBuf>> {
        use ignore::WalkBuilder;
        if matches!(self.check_path(search_path), ValidationResult::Denied) {
            anyhow::bail!("路径被沙箱拒绝：{}", search_path.display());
        }
        let mut walk = WalkBuilder::new(search_path);
        walk.standard_filters(true);
        if let Some(glob_str) = glob_filter {
            let g = glob::Pattern::new(glob_str)?;
            walk.filter_entry(move |e| {
                let name = e.file_name().to_str().unwrap_or("");
                g.matches(name) || g.matches(e.path().to_string_lossy().as_ref())
            });
        }
        let mut files = Vec::new();
        for entry in walk.build() {
            let entry = entry?;
            if !entry.file_type().is_some_and(|ft| ft.is_file()) { continue; }
            if matches!(self.check_path(entry.path()), ValidationResult::Denied) { continue; }
            files.push(entry.into_path());
        }
        Ok(files)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("路径被沙箱拒绝：{0}")]
    SandboxDenied(String),
    #[error("IO 错误：{path}，{source}")]
    Io { path: String, #[source] source: std::io::Error },
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
#[path = "workspace_test.rs"]
mod tests;
