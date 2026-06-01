use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandOutput {
    #[serde(rename = "stdout")]
    Stdout {
        text: String,
    },
    #[serde(rename = "stderr")]
    Stderr {
        text: String,
    },
    Exited {
        code: i32,
    },
    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Command {
    pub command: String,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    #[serde(skip)]
    pub cancel_token: Option<tokio_util::sync::CancellationToken>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("{0}")]
    Exec(String),
    #[error("{0}")]
    Timeout(String),
    #[error("{0}")]
    Permission(String),
}

pub trait Terminal: Send + Sync {
    fn working_dir(&self) -> PathBuf;

    fn exec(
        &self,
        cmd: Command,
    ) -> anyhow::Result<tokio::sync::mpsc::UnboundedReceiver<CommandOutput>>;
}

#[cfg(test)]
#[path = "terminal_test.rs"]
mod tests;
