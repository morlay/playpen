//! playpen acp 子命令：启动 ACP Agent。

use std::path::PathBuf;

pub fn run(cwd: &PathBuf) {
    playpen_acp::init_tracing(None);

    tracing::info!(cwd = %cwd.display(), "playpen acp 启动");

    let state = match playpen_acp::agent::build_state(cwd) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("playpen acp: 构建状态失败: {e}");
            std::process::exit(1);
        }
    };

    let rt = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
    if let Err(e) = rt.block_on(playpen_acp::agent::run(state)) {
        eprintln!("playpen acp: {e}");
        std::process::exit(1);
    }
}
