use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::select;
use tokio::time::{Duration, sleep};

use crate::terminal::{Command, CommandOutput, Terminal};

/// 进程退出后等待 stdout/stderr drain 的超时上限。
/// 命令派生的孙进程（`cmd &`、`nohup` 等）可能继续持有管道，
/// 无限等待会导致 Exited 事件永远无法送达，调用方（bash tool → tool loop）随之卡死。
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

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
            let mut spawn_cmd = TokioCommand::new("sh");
            spawn_cmd
                .arg("-c")
                .arg(&cmd.command)
                .current_dir(&cwd)
                // stdin 不继承宿主进程：ACP 模式下宿主 stdin 是协议通道，
                // 共享会导致 sh 与协议 reader 竞争抢读（消息截断 → Parse error），
                // 且交互命令（cat/read 等）永远等不到输入 → 卡死。
                // 命令默认 stdin 关闭，读入立即得到 EOF。
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                // 独立进程组：取消/超时/终止时按组 kill，覆盖命令派生的孙进程，
                // 避免孙进程残留并继续持有输出管道。
                .process_group(0)
                .kill_on_drop(true);

            let mut child = match spawn_cmd.spawn() {
                Ok(child) => child,
                Err(e) => {
                    tracing::warn!(error = %e, command = %cmd.command, "启动命令失败");
                    let _ = tx.send(CommandOutput::SpawnFailed {
                        message: format!("{e}"),
                    });
                    return;
                }
            };

            let pgid = child.id();
            tracing::debug!(command = %cmd.command, pgid, "命令已启动");

            let stdout = child.stdout.take().expect("stdout not captured");
            let stderr = child.stderr.take().expect("stderr not captured");

            let tx_stdout = tx.clone();
            let stdout_task = tokio::spawn(async move {
                let reader = BufReader::new(stdout);
                let mut lines = reader.lines();
                let mut bytes = 0usize;
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            bytes += line.len() + 1;
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
                tracing::debug!(bytes, "stdout 读取任务结束");
            });

            let tx_stderr = tx.clone();
            let stderr_task = tokio::spawn(async move {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();
                let mut bytes = 0usize;
                loop {
                    match lines.next_line().await {
                        Ok(Some(line)) => {
                            bytes += line.len() + 1;
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
                tracing::debug!(bytes, "stderr 读取任务结束");
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

            // 终止整个进程组（含命令派生的孙进程）。
            // `child.start_kill()` 只杀直接子进程（sh），孙进程仍会残留并持有管道。
            let kill_group = || {
                if let Some(pid) = pgid {
                    tracing::debug!(pgid, "kill 进程组");
                    // 进程组 id == 组长 pid（process_group(0) 时成立）
                    unsafe {
                        libc::kill(-(pid as i32), libc::SIGKILL);
                    }
                }
            };

            match term {
                Term::Exited(status) => {
                    // 进程自然退出：先等输出读完再发 Exited，保证输出顺序。
                    // 但孙进程可能仍持有管道导致 EOF 永不出现，超时后强制终止
                    // 残留进程组并发送终态，保证调用方不卡死。
                    let code = status.map(&exit_code).unwrap_or(-1);
                    let drained = tokio::time::timeout(DRAIN_TIMEOUT, async {
                        let _ = stdout_task.await;
                        let _ = stderr_task.await;
                    })
                    .await;
                    if drained.is_err() {
                        tracing::warn!(
                            command = %cmd.command,
                            "命令已退出但输出 drain 超时（疑似孙进程持有管道），强制终止进程组"
                        );
                        kill_group();
                        let _ = child.wait().await;
                    }
                    let _ = tx.send(CommandOutput::Exited { code });
                }
                // 取消/超时：子进程可能仍在运行，kill 整个进程组后管道 EOF，
                // 读取任务会很快结束；等它们收尾保证终止前的输出先于终态事件送达
                Term::Cancelled | Term::TimedOut => {
                    kill_group();
                    let _ = child.wait().await;
                    let drained = tokio::time::timeout(DRAIN_TIMEOUT, async {
                        let _ = stdout_task.await;
                        let _ = stderr_task.await;
                    })
                    .await;
                    if drained.is_err() {
                        tracing::warn!(
                            command = %cmd.command,
                            "命令终止后输出 drain 超时，跳过剩余输出"
                        );
                    }
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
