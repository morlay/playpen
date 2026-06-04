//! 文件持久化的 Session 存储。
//!
//! 封装 `SessionManager`（内存），启动时从 `{store_dir}/*.json`
//! 恢复已有会话，每次变更后写回文件。

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use playpen_agent_core::model::Model;
use playpen_agent_core::session::message::Message;
use playpen_agent_core::session::session::create_session;
use playpen_agent_core::session::store::SessionManager;
use playpen_agent_core::tool::ToolSchema;

/// 可序列化的持久化 session 结构。
#[derive(serde::Serialize, serde::Deserialize)]
struct PersistedSession {
    id: String,
    title: String,
    model: Model,
    project_root: String,
    agent_name: String,
    system_prompt: String,
    tools_schema: Vec<ToolSchema>,
    messages: Vec<Message>,
    acc_cost: f64,
    total_tokens: Option<usize>,
    created_at: chrono::DateTime<chrono::Utc>,
    archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 文件持久化的 Session 存储。
pub struct FileBackedSessionStore {
    manager: Arc<SessionManager>,
    store_dir: PathBuf,
}

impl FileBackedSessionStore {
    /// 创建新的持久化存储。启动时从 `store_dir` 加载已有会话恢复到内存。
    pub fn new(store_dir: PathBuf, manager: Arc<SessionManager>) -> anyhow::Result<Self> {
        fs::create_dir_all(&store_dir)?;

        if let Ok(entries) = fs::read_dir(&store_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json")
                    && let Ok(data) = fs::read_to_string(&path)
                        && let Ok(ps) = serde_json::from_str::<PersistedSession>(&data) {
                            tracing::debug!(
                                session_id = %ps.id,
                                messages = ps.messages.len(),
                                "从文件恢复会话"
                            );
                            let mut session = create_session(
                                ps.title.clone(),
                                ps.model.clone(),
                                PathBuf::from(&ps.project_root),
                                ps.agent_name.clone(),
                                ps.system_prompt.clone(),
                                ps.tools_schema.clone(),
                                Some(ps.model.context_window),
                            );
                            session.id = ps.id.clone();
                            session.messages = ps.messages.clone();
                            session.acc_cost = ps.acc_cost;
                            session.total_tokens = ps.total_tokens;
                            session.created_at = ps.created_at;
                            session.archived_at = ps.archived_at;
                            manager.insert(session);
                        }
            }
        }

        tracing::info!(
            dir = %store_dir.display(),
            "Session 持久化存储初始化完成"
        );

        Ok(Self { manager, store_dir })
    }

    /// 获取内部 SessionManager 引用。
    pub fn manager(&self) -> Arc<SessionManager> {
        self.manager.clone()
    }

    /// 创建新 session 并持久化。
    pub fn create(
        &self,
        title: &str,
        model: Model,
        project_root: &str,
        agent_name: &str,
        system_prompt: &str,
        tools_schema: Vec<ToolSchema>,
        context_window: Option<usize>,
    ) -> String {
        let session = create_session(
            title.into(),
            model,
            PathBuf::from(project_root),
            agent_name.into(),
            system_prompt.into(),
            tools_schema,
            context_window,
        );
        let id = session.id.clone();
        self.manager.insert(session);
        self.persist(&id);
        id
    }

    /// 持久化指定 session 到文件。
    pub fn persist(&self, id: &str) {
        let session = match self.manager.get(id) {
            Some(s) => s,
            None => {
                tracing::warn!(session_id = %id, "persist: session 未找到");
                return;
            }
        };
        let persisted = PersistedSession {
            id: session.id.clone(),
            title: session.title.clone(),
            model: session.model.clone(),
            project_root: session.project_root.to_string_lossy().to_string(),
            agent_name: session.agent_name.clone(),
            system_prompt: session.system_prompt.clone(),
            tools_schema: session.tools_schema.clone(),
            messages: session.messages.clone(),
            acc_cost: session.acc_cost,
            total_tokens: session.total_tokens,
            created_at: session.created_at,
            archived_at: session.archived_at,
        };
        let path = self.store_dir.join(format!("{}.json", id));
        match serde_json::to_string_pretty(&persisted) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, &json) {
                    tracing::error!(path = %path.display(), error = %e, "持久化写入失败");
                }
            }
            Err(e) => {
                tracing::error!(session_id = %id, error = %e, "持久化序列化失败");
            }
        }
    }
}
