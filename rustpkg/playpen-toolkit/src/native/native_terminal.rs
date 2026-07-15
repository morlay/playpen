use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::select;

use crate::terminal::{Command, CommandOutput, Terminal};

pub struct NativeTerminal;

impl Terminal for NativeTerminal {
    fn working_dir(&self) -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    fn exec(
        &self,
        cmd: Command,
    ) -> anyhow::Result<tokio::sync::mpsc::UnboundedReceiver<CommandOutput>> {
        let cwd = cmd
            .cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_dir());

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancel_token = cmd.cancel_token.clone();

        tokio::spawn(async move {
            let mut child = match TokioCommand::new("sh")
                .arg("-c")
                .arg(&cmd.command)
                .current_dir(&cwd)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()
            {
                Ok(child) => child,
                Err(e) => {
                    tracing::warn!(error = %e, command = %cmd.command, "启动命令失败");
                    let _ = tx.send(CommandOutput::SpawnFailed {
                        message: format!("{e}"),
                    });
                    return;
                }
            };

            let stdout = child.stdout.take().expect("stdout not captured");
            let stderr = child.stderr.take().expect("stderr not captured");

            let tx_stdout = tx.clone();
            let stdout_task = tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            if tx_stdout
                                .send(CommandOutput::Stdout {
                                    text: format!("{line}\n"),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(error = %e, "读取 stdout 失败");
                            break;
                        }
                    }
                }
            });

            let tx_stderr = tx.clone();
            let stderr_task = tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            if tx_stderr
                                .send(CommandOutput::Stderr {
                                    text: format!("{line}\n"),
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(error = %e, "读取 stderr 失败");
                            break;
                        }
                    }
                }
            });

            // 先等所有输出 drain 完，再发退出信号
            let _ = stdout_task.await;
            let _ = stderr_task.await;

            let exit_code = |status: std::process::ExitStatus| status.code().unwrap_or(-1);

            if let Some(token) = cancel_token {
                select! {
                    status = child.wait() => {
                        let code = status.map(&exit_code).unwrap_or(-1);
                        let _ = tx.send(CommandOutput::Exited { code });
                    }
                    _ = token.cancelled() => {
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                        let _ = tx.send(CommandOutput::Cancelled);
                    }
                }
            } else {
                let status = child.wait().await;
                let code = status.map(&exit_code).unwrap_or(-1);
                let _ = tx.send(CommandOutput::Exited { code });
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
#[path = "native_terminal_test.rs"]
mod tests;
