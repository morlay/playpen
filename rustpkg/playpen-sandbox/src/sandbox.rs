use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Verdict {
    Allowed,
    ReadOnly,
    Denied,
}

#[derive(Debug, Clone)]
pub struct AccessVerdict {
    pub verdict: Verdict,
    pub url: String,
}

impl AccessVerdict {
    pub fn new(verdict: Verdict, url: impl Into<String>) -> Self {
        Self {
            verdict,
            url: url.into(),
        }
    }

    pub fn allowed(url: impl Into<String>) -> Self {
        Self::new(Verdict::Allowed, url)
    }

    pub fn readonly(url: impl Into<String>) -> Self {
        Self::new(Verdict::ReadOnly, url)
    }

    pub fn denied(url: impl Into<String>) -> Self {
        Self::new(Verdict::Denied, url)
    }
}

pub struct Command {
    pub command: String,
    pub cwd: Option<PathBuf>,
    pub env: HashMap<String, String>,
}

impl Command {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            cwd: None,
            env: HashMap::new(),
        }
    }

    pub fn from_args(args: &[String]) -> Result<Self, String> {
        let command = crate::config::shell::join_args(args)?;
        Ok(Self::new(command))
    }

    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Unexpected(String),
}

pub trait Sandbox: Send + Sync {
    fn access(&self, uri: &str) -> AccessVerdict;
    fn wrap_command(&self, cmd: Command) -> Result<Command, Error>;
}

#[cfg(test)]
#[path = "sandbox_test.rs"]
mod tests;
