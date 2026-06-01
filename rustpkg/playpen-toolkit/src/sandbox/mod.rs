use std::path::Path;

use crate::Toolkit;

mod fs;
mod terminal;

pub use fs::SandboxFileSystem;
pub use terminal::SandboxTerminal;

/// 将 `playpen_config::SandboxProfile` 映射为 `playpen_sandbox::config::Config`。
fn sandbox_config_from_profile(
    p: &playpen_config::SandboxProfile,
) -> playpen_sandbox::config::Config {
    playpen_sandbox::config::Config {
        network: p
            .network
            .as_ref()
            .map(|a| playpen_sandbox::config::AllowSection {
                access: a.access.clone(),
            }),
        filesystem: p
            .filesystem
            .as_ref()
            .map(|a| playpen_sandbox::config::AllowSection {
                access: a.access.clone(),
            }),
        shell: p
            .shell
            .as_ref()
            .map(|s| playpen_sandbox::config::ShellSection {
                allow_pipe: s.allow_pipe,
                allow_multiple: s.allow_multiple,
                allow: s.allow.clone(),
            }),
    }
}

impl Toolkit {
    /// 根据 sandbox profile 配置叠加沙箱包装层。
    /// 仅在 `sandbox` feature 启用时可用。
    pub fn with_sandbox_profile(
        self,
        profile: &playpen_config::SandboxProfile,
        cwd: &Path,
    ) -> Self {
        let config = sandbox_config_from_profile(profile);
        let sandbox = playpen_sandbox::create(&config, cwd);
        self.with_sandbox(sandbox)
    }
}
