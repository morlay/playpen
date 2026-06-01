use std::path::PathBuf;

use tracing_subscriber::fmt::format::FmtSpan;

pub fn init_logging() {
    let log_dir = std::env::var("PLAYPEN_LOG_DIR").ok();
    let default_level = if log_dir.is_some() { "info" } else { "warn" };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

    match log_dir {
        Some(dir) => {
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("[playpen] 创建日志目录失败: {e}");
            }

            let ts = chrono::Local::now().format("%Y%m%dT%H%M%S");
            let log_path = PathBuf::from(&dir).join(format!("playpen-{ts}.jsonl"));

            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .unwrap();

            tracing_subscriber::fmt()
                .json()
                .with_writer(std::sync::Mutex::new(file))
                .with_timer(tracing_subscriber::fmt::time::SystemTime)
                .with_span_events(FmtSpan::CLOSE)
                .with_env_filter(filter)
                .try_init()
                .ok();
        }
        None => {
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_span_events(FmtSpan::CLOSE)
                .with_env_filter(filter)
                .try_init()
                .ok();
        }
    }
}
