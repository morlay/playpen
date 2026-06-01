use super::*;
use crate::terminal::{Command, CommandOutput, Terminal};
use std::sync::Arc;

use crate::native::NativeTerminal;

struct AllowAll;
impl playpen_sandbox::Sandbox for AllowAll {
    fn access(&self, uri: &str) -> playpen_sandbox::AccessVerdict {
        playpen_sandbox::AccessVerdict::allowed(uri)
    }
    fn wrap_command(
        &self,
        cmd: playpen_sandbox::Command,
    ) -> Result<playpen_sandbox::Command, playpen_sandbox::Error> {
        Ok(cmd)
    }
}

#[tokio::test]
async fn exec_through_sandbox() {
    let term = SandboxTerminal::new(Arc::new(AllowAll), Arc::new(NativeTerminal));
    let mut rx = term
        .exec(Command {
            command: "echo hello".into(),
            ..Default::default()
        })
        .unwrap();
    let mut results = Vec::new();
    while let Some(item) = rx.recv().await {
        results.push(item);
    }
    assert!(!results.is_empty());
    assert!(matches!(&results[0], CommandOutput::Stdout { text } if text.trim() == "hello"));
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Exited { .. }))
    );
}
