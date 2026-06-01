use std::str::FromStr;
use std::sync::Arc;

use agent_client_protocol::Stdio;
use playpen_config::{Dirs, Settings};
use playpen_session::DBSessionService;

pub async fn run(cwd: &std::path::Path, settings: &Settings) {
    crate::log::init_logging();
    let dirs = Dirs::with_defaults(cwd);

    let db_path = crate::db::sessions_db_path();
    let db_url = format!("sqlite://{}", db_path.display());
    let pool = match sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::from_str(&db_url)
            .expect("无效的 session 数据库 URL")
            .create_if_missing(true),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ACP 启动失败: {e}");
            return;
        }
    };
    let session_svc = DBSessionService::new(pool.into());
    if let Err(e) = session_svc.migrate().await {
        eprintln!("ACP 启动失败: {e}");
        return;
    }
    let session_service: Arc<dyn playpen_session::SessionService> = Arc::new(session_svc);

    let profile_resolver: Arc<dyn playpen_profile::AgentProfileLoader> =
        Arc::new(playpen_profile::LocalAgentProfileLoader);

    let builder =
        playpen_agent::SimpleRunnerBuilder::new(settings, &dirs, session_service, profile_resolver);

    if let Err(e) = playpen_acp::serve(Box::new(builder), Stdio::new()).await {
        eprintln!("ACP 启动失败: {e}");
    }
}
