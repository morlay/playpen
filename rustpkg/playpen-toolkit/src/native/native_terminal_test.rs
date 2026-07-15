use std::time::Duration;

use super::*;
use crate::terminal::{Command, CommandOutput, Terminal};

#[tokio::test]
async fn exec_echo() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "echo hello".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();
    assert!(!results.is_empty());
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Stdout { text } if text.trim() == "hello"))
    );
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Exited { .. }))
    );
}

#[tokio::test]
async fn exec_stderr() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "echo error >&2".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Stderr { text } if text.contains("error")))
    );
}

#[tokio::test]
async fn exec_command_fails() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "nonexistent_cmd_xyz".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Stderr { .. }))
    );
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Exited { code } if *code != 0))
    );
}

#[tokio::test]
async fn exec_exit_code_nonzero() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "exit 1".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Exited { code: 1 }))
    );
}

#[tokio::test]
async fn exec_cancel() {
    let term = NativeTerminal;
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let cmd = Command {
        command: "sleep 30".into(),
        cancel_token: Some(cancel_token.clone()),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();

    // 延迟取消，确保命令已启动
    let ct = cancel_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        ct.cancel();
    });

    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Cancelled)),
        "应收到 Cancelled，实际: {:?}",
        results,
    );
}

#[tokio::test]
async fn exec_large_output() {
    let term = NativeTerminal;
    // 产生约 700KB 输出，远超默认 pipe buffer（64KB）
    let cmd = Command {
        command: "seq 100000".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    let stdout_lines: usize = results
        .iter()
        .filter(|r| matches!(r, CommandOutput::Stdout { .. }))
        .count();
    assert!(
        stdout_lines > 1000,
        "大量输出应拆分为多行 stdout，实际 {stdout_lines} 行"
    );
    assert!(
        results
            .iter()
            .any(|r| matches!(r, CommandOutput::Exited { code: 0 })),
        "应收到 Exited(code=0)"
    );
}

#[tokio::test]
async fn exec_custom_cwd() {
    let dir = tempfile::tempdir().unwrap();
    let dir_path = dir.path().canonicalize().unwrap();
    let term = NativeTerminal;
    let cmd = Command {
        command: "pwd".into(),
        cwd: Some(dir_path.to_string_lossy().to_string()),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    assert!(
        results.iter().any(|r| matches!(r, CommandOutput::Stdout { text } if text.trim() == dir_path.to_string_lossy())),
        "pwd 应输出自定义工作目录 {:?}，实际: {:?}",
        dir_path,
        results,
    );
}

/// 无输出直接 exit 0
#[tokio::test]
async fn exec_no_output_exit_zero() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "exit 0".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results: Vec<CommandOutput> = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    // 只有 Exited，没有 Stdout/Stderr
    assert!(
        results.iter().all(|r| matches!(r, CommandOutput::Exited { .. })),
        "无输出命令只应产生 Exited，实际: {:?}",
        results,
    );
    assert!(
        results.iter().any(|r| matches!(r, CommandOutput::Exited { code: 0 })),
        "exit code 应为 0"
    );
}

/// 无输出直接 exit 非零
#[tokio::test]
async fn exec_no_output_exit_nonzero() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "exit 42".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results: Vec<CommandOutput> = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    // 只有 Exited，没有 Stdout/Stderr
    assert_eq!(results.len(), 1, "无输出命令只应产生 1 个事件");
    assert!(
        matches!(&results[0], CommandOutput::Exited { code: 42 }),
        "exit code 应为 42，实际: {:?}",
        results,
    );
}

/// 所有 Stdout 消息必须在 Exited 之前（消息顺序）
#[tokio::test]
async fn exec_message_order_stdout_before_exit() {
    let term = NativeTerminal;
    let cmd = Command {
        command: "printf 'a\nb\nc'".into(),
        ..Default::default()
    };
    let mut rx = term.exec(cmd).unwrap();
    let results: Vec<CommandOutput> = tokio::task::spawn_blocking(move || {
        let mut v = Vec::new();
        while let Some(item) = rx.blocking_recv() {
            v.push(item);
        }
        v
    })
    .await
    .unwrap();

    // 找到第一个 Exited 的位置
    let first_exit_idx = results
        .iter()
        .position(|r| matches!(r, CommandOutput::Exited { .. }))
        .expect("应有 Exited");

    // Exited 之前的所有消息都必须是 Stdout
    for item in &results[..first_exit_idx] {
        assert!(
            matches!(item, CommandOutput::Stdout { .. }),
            "Exited 之前的消息必须是 Stdout，实际: {:?}",
            item,
        );
    }

    // Exited 之后不应再有其他消息
    for item in &results[first_exit_idx + 1..] {
        assert!(
            matches!(item, CommandOutput::Exited { .. }),
            "Exited 之后不应有其他消息，实际: {:?}",
            item,
        );
    }
}


