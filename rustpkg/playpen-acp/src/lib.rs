pub mod event_mapping;
pub mod session_store;

pub mod agent;
mod handler;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// 初始化 tracing 订阅器。重复调用无副作用。
///
/// 日志同时输出到两个目标：
/// - stderr：人类可读格式，便于本地调试
/// - 文件（`<log_dir>/playpen-acp-{ts}.log`）：JSON 格式，便于机器解析
///
/// 设置环境变量 `PLAYPEN_LOG_DIR` 启用文件日志。
pub fn init_tracing(log_dir: Option<&PathBuf>) {
    use tracing_subscriber::filter::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,playpen_acp=debug"));

    let stderr_layer = fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr);

    if let Some(dir) = log_dir {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("创建日志目录失败 {}: {e}", dir.display());
            tracing_subscriber::registry()
                .with(stderr_layer)
                .with(filter)
                .try_init()
                .ok();
            return;
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let log_path = dir.join(format!("playpen-acp-{ts}.log"));
        match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => {
                let file_layer = fmt::layer()
                    .json()
                    .with_writer(file);
                tracing_subscriber::registry()
                    .with(stderr_layer)
                    .with(file_layer)
                    .with(filter)
                    .try_init()
                    .ok();
            }
            Err(e) => {
                eprintln!("无法打开日志文件 {}: {e}", log_path.display());
                tracing_subscriber::registry()
                    .with(stderr_layer)
                    .with(filter)
                    .try_init()
                    .ok();
            }
        }
    } else {
        tracing_subscriber::registry()
            .with(stderr_layer)
            .with(filter)
            .try_init()
            .ok();
    }
}
