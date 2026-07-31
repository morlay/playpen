use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::select;
use tokio::time::{Duration, sleep};

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

            let exit_code = |status: std::process::ExitStatus| status.code().unwrap_or(-1);

            /// 进程终止原因：自然退出 / 取消 / 超时
            enum Term {
                Exited(std::io::Result<std::process::ExitStatus>),
                Cancelled,
                TimedOut,
            }

            // 等待进程结束、取消或超时。取消/超时必须立即响应：不能在等待输出
            // drain 之后再检查，否则无输出的长驻命令（如 sleep）会让终止永远
            // 无法生效，导致调用方（bash tool → tool loop）阻塞直到进程自然退出。
            let timeout = cmd.timeout_ms.map(Duration::from_millis);
            let term = match (&cancel_token, timeout) {
                (Some(token), Some(t)) => select! {
                    status = child.wait() => Term::Exited(status),
                    _ = token.cancelled() => Term::Cancelled,
                    _ = sleep(t) => Term::TimedOut,
                },
                (Some(token), None) => select! {
                    status = child.wait() => Term::Exited(status),
                    _ = token.cancelled() => Term::Cancelled,
                },
                (None, Some(t)) => select! {
                    status = child.wait() => Term::Exited(status),
                    _ = sleep(t) => Term::TimedOut,
                },
                (None, None) => Term::Exited(child.wait().await),
            };

            // 非正常终止（取消/超时）对应的终态事件
            let forced = match &term {
                Term::Cancelled => Some(CommandOutput::Cancelled),
                Term::TimedOut => Some(CommandOutput::Timeout),
                _ => None,
            };

            match term {
                Term::Exited(status) => {
                    // 进程自然退出：先等输出读完再发 Exited，保证输出顺序
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    let code = status.map(&exit_code).unwrap_or(-1);
                    let _ = tx.send(CommandOutput::Exited { code });
                }
                // 取消/超时：子进程可能仍在运行，kill 后管道 EOF，读取任务会很快
                // 结束；等它们收尾保证终止前的输出先于终态事件送达
                Term::Cancelled | Term::TimedOut => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    let _ = tx.send(forced.expect("非正常终止必有终态事件"));
                }
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
#[path = "native_terminal_test.rs"]
mod tests;
