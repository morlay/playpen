use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use futures::Future;
use rig_core::memory::{ConversationMemory, MemoryError};
use rusqlite::Connection;

/// 基于 SQLite 的消息记忆，数据库文件存储在 `$XDG_DATA_HOME/playpen/sessions/memory.db`
#[derive(Clone)]
pub struct SqliteMemory {
    pub(crate) conn: Arc<std::sync::Mutex<Connection>>,
}

impl SqliteMemory {
    pub fn open(db_path: &std::path::Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                conversation_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                content TEXT NOT NULL,
                PRIMARY KEY (conversation_id, seq)
            )"
        )?;
        Ok(Self { conn: Arc::new(std::sync::Mutex::new(conn)) })
    }

    /// 默认路径：$XDG_DATA_HOME/playpen/sessions/memory.db
    pub fn open_default() -> anyhow::Result<Self> {
        let dir = xdg_data_home().join("playpen").join("sessions");
        std::fs::create_dir_all(&dir)?;
        Self::open(&dir.join("memory.db"))
    }
}

fn xdg_data_home() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".local").join("share")
        })
}

impl ConversationMemory for SqliteMemory {
    fn load<'a>(&'a self, conversation_id: &'a str) -> Pin<Box<dyn Future<Output = Result<Vec<rig_core::completion::Message>, MemoryError>> + Send + 'a>> {
        Box::pin(async move {
            let conn = self.conn.lock().map_err(|_| MemoryError::Internal("锁错误".into()))?;
            let mut stmt = conn.prepare("SELECT content FROM messages WHERE conversation_id = ?1 ORDER BY seq")
                .map_err(|e| MemoryError::Internal(e.to_string()))?;
            let messages: Vec<rig_core::completion::Message> = stmt
                .query_map([conversation_id], |row| row.get::<_, String>(0))
                .map_err(|e| MemoryError::Internal(e.to_string()))?
                .filter_map(|r| r.ok())
                .filter_map(|json| serde_json::from_str(&json).ok())
                .collect();
            Ok(messages)
        })
    }

    fn append<'a>(&'a self, conversation_id: &'a str, messages: Vec<rig_core::completion::Message>) -> Pin<Box<dyn Future<Output = Result<(), MemoryError>> + Send + 'a>> {
        Box::pin(async move {
            let conn = self.conn.lock().map_err(|_| MemoryError::Internal("锁错误".into()))?;
            let mut stmt = conn.prepare("SELECT COALESCE(MAX(seq), -1) FROM messages WHERE conversation_id = ?1")
                .map_err(|e| MemoryError::Internal(e.to_string()))?;
            let max_seq: i64 = stmt.query_row([conversation_id], |row| row.get(0))
                .map_err(|e| MemoryError::Internal(e.to_string()))?;
            let seq = max_seq + 1;
            for (i, msg) in messages.iter().enumerate() {
                let json = serde_json::to_string(msg).map_err(|e| MemoryError::Internal(e.to_string()))?;
                conn.execute(
                    "INSERT INTO messages (conversation_id, seq, content) VALUES (?1, ?2, ?3)",
                    rusqlite::params![conversation_id, seq + i as i64, json],
                ).map_err(|e| MemoryError::Internal(e.to_string()))?;
            }
            Ok(())
        })
    }

    fn clear<'a>(&'a self, conversation_id: &'a str) -> Pin<Box<dyn Future<Output = Result<(), MemoryError>> + Send + 'a>> {
        Box::pin(async move {
            let conn = self.conn.lock().map_err(|_| MemoryError::Internal("锁错误".into()))?;
            conn.execute("DELETE FROM messages WHERE conversation_id = ?1", [conversation_id])
                .map_err(|e| MemoryError::Internal(e.to_string()))?;
            Ok(())
        })
    }
}

#[cfg(test)]
#[path = "memory_test.rs"]
mod tests;
