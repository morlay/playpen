use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use playpen_content::{ContentBlock, Event};
use playpen_toolkit::terminal::{Command, CommandOutput, Terminal};

use crate::tool::{BashTool, Tool, ToolContext};

/// Mock terminal 模拟命令执行输出序列。
struct MockTerminal {
    steps: Vec<CommandOutput>,
}

#[async_trait]
impl Terminal for MockTerminal {
    fn working_dir(&self) -> PathBuf {
        PathBuf::from("/tmp")
    }

    fn exec(
        &self,
        _cmd: Command,
    ) -> anyhow::Result<tokio::sync::mpsc::UnboundedReceiver<CommandOutput>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let steps = self.steps.clone();
        tokio::spawn(async move {
            for step in steps {
                let _ = tx.send(step);
            }
            // tx 在此 drop，channel 关闭
        });
        Ok(rx)
    }
}

fn make_tool(steps: Vec<CommandOutput>) -> BashTool {
    BashTool {
        term: Arc::new(MockTerminal { steps }),
    }
}

fn make_ctx() -> (ToolContext, mpsc::UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let ctx = ToolContext::new(
        "event-1".to_string(),
        "call-1".to_string(),
        "bash".to_string(),
        tx,
        CancellationToken::new(),
    );
    (ctx, rx)
}

/// 从 blocks 中提取 Resource::Text 的文本内容
fn resource_texts(blocks: &[ContentBlock]) -> Vec<&str> {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Resource(playpen_content::Resource::Text { text, .. }) => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect()
}

/// 正常执行：stdout → Exited(0)
#[tokio::test]
async fn test_stdout_then_exit() {
    let tool = make_tool(vec![
        CommandOutput::Stdout {
            text: "hello\n".into(),
        },
        CommandOutput::Exited { code: 0 },
    ]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "echo hello"}))
        .await
        .unwrap();

    let texts = resource_texts(&blocks);
    assert!(texts.iter().any(|t| t.contains("hello")), "stdout 应出现");
}

/// 收到 Exited 后立即退出，不等待更多输出
#[tokio::test]
async fn test_exit_breaks_loop() {
    let tool = make_tool(vec![
        CommandOutput::Stdout {
            text: "output\n".into(),
        },
        CommandOutput::Exited { code: 0 },
        // Exited 后的内容不应被处理（模拟 channel 延迟关闭的场景）
        CommandOutput::Stderr {
            text: "should not appear\n".into(),
        },
    ]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "test"}))
        .await
        .unwrap();

    let texts = resource_texts(&blocks);
    assert!(
        texts.iter().any(|t| t.contains("output")),
        "stdout 内容应出现"
    );
    assert!(
        !texts.iter().any(|t| t.contains("should not appear")),
        "Exited 后的输出不应被处理"
    );
}

/// 收到 Cancelled 后立即退出
#[tokio::test]
async fn test_cancelled_breaks_loop() {
    let tool = make_tool(vec![
        CommandOutput::Stdout {
            text: "partial\n".into(),
        },
        CommandOutput::Cancelled,
        CommandOutput::Stderr {
            text: "should not appear\n".into(),
        },
    ]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "test"}))
        .await
        .unwrap();

    let texts = resource_texts(&blocks);
    assert!(
        texts.iter().any(|t| t.contains("partial")),
        "取消前的输出应保留"
    );
    assert!(
        !texts.iter().any(|t| t.contains("should not appear")),
        "取消后的输出不应被处理"
    );

    // 应有"命令执行已被取消"提示
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::Text(t) if t.text.contains("取消"))),
        "应有取消提示"
    );
}

/// 命令无输出 + 有退出码 → 返回"执行成功（无输出）"
#[tokio::test]
async fn test_no_output() {
    let tool = make_tool(vec![CommandOutput::Exited { code: 0 }]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "silent"}))
        .await
        .unwrap();

    assert!(
        blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::Text(t) if t.text.contains("执行成功（无输出）"))),
        "无输出时应返回提示"
    );
}

/// 同时有 stdout 和 stderr
#[tokio::test]
async fn test_stdout_and_stderr() {
    let tool = make_tool(vec![
        CommandOutput::Stdout {
            text: "hello\n".into(),
        },
        CommandOutput::Stderr {
            text: "warning\n".into(),
        },
        CommandOutput::Exited { code: 1 },
    ]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "mixed"}))
        .await
        .unwrap();

    let texts = resource_texts(&blocks);
    assert!(texts.iter().any(|t| t.contains("hello")), "stdout 应出现");
    assert!(texts.iter().any(|t| t.contains("warning")), "stderr 应出现");
}

/// exit_code 注解在最后 block
#[tokio::test]
async fn test_exit_code_annotation() {
    let tool = make_tool(vec![
        CommandOutput::Stderr {
            text: "错误\n".into(),
        },
        CommandOutput::Exited { code: 1 },
    ]);
    let (ctx, _rx) = make_ctx();
    let blocks = tool
        .execute(ctx, serde_json::json!({"command": "fail"}))
        .await
        .unwrap();

    let last = blocks.last().unwrap();
    match last {
        ContentBlock::Resource(r) => match r {
            playpen_content::Resource::Text { annotations, .. } => {
                let a = annotations.as_ref().expect("应有 annotations");
                assert_eq!(
                    a.get("exit_code").and_then(|v| v.as_i64()),
                    Some(1),
                    "exit_code=1"
                );
            }
            _ => panic!("期望 Resource::Text"),
        },
        _ => panic!("期望 ContentBlock::Resource"),
    }
}

/// SpawnFailed 应向上传播为 error，不吞没
#[tokio::test]
async fn test_spawn_failed_propagates_error() {
    let tool = make_tool(vec![CommandOutput::SpawnFailed {
        message: "sh: command not found".into(),
    }]);
    let (ctx, _rx) = make_ctx();
    let err = tool
        .execute(ctx, serde_json::json!({"command": "whatever"}))
        .await
        .expect_err("SpawnFailed 应返回错误");

    let msg = format!("{err}");
    assert!(
        msg.contains("sh: command not found"),
        "错误信息应包含原始消息，实际: {msg}"
    );
}
