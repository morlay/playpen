use std::io::Write;

use futures::StreamExt;
use playpen_agent::AgentRunnerBuilder;
use playpen_session::SessionStats;

/// 向 stdout 写入一行，BrokenPipe 时静默退出。
fn println_stdout(s: &str) {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    match writeln!(handle, "{s}") {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("写入 stdout 失败: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn get(builder: &dyn AgentRunnerBuilder, id: &str) {
    let svc = builder.sessions();

    match svc.get(id).await {
        Ok(session) => {
            let events = session.events().all().await.collect::<Vec<_>>().await;

            for event in &events {
                println_stdout(&serde_json::to_string(&event).unwrap_or_default());
            }

            // 最后打印统计
            let stats = SessionStats::from_events(&events);
            println_stdout(&serde_json::to_string(&stats).unwrap_or_default());
        }
        Err(e) => eprintln!("查询 session 失败: {e}"),
    }
}

pub async fn list(builder: &dyn AgentRunnerBuilder, limit: Option<usize>, offset: usize) {
    let svc = builder.sessions();

    match svc.list(limit, offset).await {
        Ok(sessions) => {
            let total = sessions.len();
            for session in &sessions {
                println_stdout(
                    &serde_json::to_string(&serde_json::json!({
                        "id": session.id(),
                    }))
                    .unwrap_or_default(),
                );
            }
            eprintln!(
                "  (显示 {} 条, offset={})",
                total.min(limit.unwrap_or(total)),
                offset
            );
        }
        Err(e) => eprintln!("查询 session 列表失败: {e}"),
    }
}
