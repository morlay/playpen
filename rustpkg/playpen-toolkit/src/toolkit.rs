use std::path::Path;
use std::sync::Arc;

use crate::fetch::Fetcher;
use crate::fs::FileSystem;
use crate::native::{NativeFetcher, NativeFileSystem, NativeTerminal};
use crate::terminal::Terminal;

pub struct Toolkit {
    pub file_system: Arc<dyn FileSystem>,
    pub terminal: Arc<dyn Terminal>,
    pub fetcher: Arc<dyn Fetcher>,
}

impl Toolkit {
    pub fn defaults(cwd: &Path) -> Self {
        Self {
            file_system: Arc::new(NativeFileSystem::new(cwd.to_path_buf())),
            terminal: Arc::new(NativeTerminal),
            fetcher: Arc::new(NativeFetcher),
        }
    }

    #[cfg(feature = "sandbox")]
    pub fn with_sandbox(self, sandbox: std::sync::Arc<dyn playpen_sandbox::Sandbox>) -> Self {
        use crate::sandbox::{SandboxFileSystem, SandboxTerminal};
        Self {
            file_system: Arc::new(SandboxFileSystem::new(self.file_system, sandbox.clone())),
            terminal: Arc::new(SandboxTerminal::new(sandbox, self.terminal)),
            fetcher: self.fetcher,
        }
    }
}

#[cfg(test)]
#[path = "toolkit_test.rs"]
mod tests;
