use std::path::PathBuf;
use std::sync::Arc;

use playpen_sandbox::Sandbox;

use crate::terminal::{Command, CommandOutput, Terminal};

pub struct SandboxTerminal {
    sandbox: Arc<dyn Sandbox>,
    inner: Arc<dyn Terminal>,
}

impl SandboxTerminal {
    pub fn new(sandbox: Arc<dyn Sandbox>, inner: Arc<dyn Terminal>) -> Self {
        Self { sandbox, inner }
    }
}

impl Terminal for SandboxTerminal {
    fn working_dir(&self) -> PathBuf {
        self.inner.working_dir()
    }

    fn exec(
        &self,
        cmd: Command,
    ) -> anyhow::Result<tokio::sync::mpsc::UnboundedReceiver<CommandOutput>> {
        let mut sandbox_cmd = playpen_sandbox::Command::new(cmd.command.clone());
        sandbox_cmd.cwd = cmd.cwd.as_deref().map(PathBuf::from);

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let approved = match self.sandbox.wrap_command(sandbox_cmd) {
            Ok(approved) => approved,
            Err(e) => {
                let _ = tx.send(CommandOutput::Stderr {
                    text: format!("{}\n", e),
                });
                let _ = tx.send(CommandOutput::Exited { code: 1 });
                return Ok(rx);
            }
        };

        let wrapped = Command {
            command: approved.command,
            cwd: approved.cwd.map(|p| p.display().to_string()).or(cmd.cwd),
            ..cmd
        };

        self.inner.exec(wrapped)
    }
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod terminal_test;
