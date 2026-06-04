use std::collections::HashMap;
use std::sync::Mutex;

use crate::session::session::Session;
use crate::session::message::Message;

/// Session 管理器。所有操作线程安全。
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Session>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self { sessions: Mutex::new(HashMap::new()) }
    }

    pub fn get(&self, id: &str) -> Option<Session> {
        self.sessions.lock().ok()?.get(id).cloned()
    }

    pub fn insert(&self, session: Session) {
        if let Ok(mut s) = self.sessions.lock() {
            s.insert(session.id.clone(), session);
        }
    }

    pub fn list(&self) -> Vec<Session> {
        let mut sessions: Vec<_> = match self.sessions.lock() {
            Ok(g) => g.values().cloned().collect(),
            Err(_) => return vec![],
        };
        sessions.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        sessions
    }

    pub fn archive(&self, id: &str) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        if s.archived_at.is_some() {
            anyhow::bail!("会话 {} 已归档", id);
        }
        s.archived_at = Some(chrono::Utc::now());
        Ok(())
    }

    pub fn delete(&self, id: &str) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        if s.archived_at.is_none() {
            anyhow::bail!("仅允许删除已归档的会话");
        }
        sessions.remove(id);
        Ok(())
    }

    pub fn update_title(&self, id: &str, title: &str) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        s.title = title.to_string();
        Ok(())
    }

    pub fn update_model(&self, id: &str, model: crate::model::Model) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        s.context_window = Some(model.context_window);
        s.model = model;
        Ok(())
    }

    pub fn fork(&self, id: &str, agent_name: &str) -> anyhow::Result<String> {
        let sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let source = sessions.get(id).cloned().ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        drop(sessions);
        let mut new = crate::session::session::create_session(
            format!("{} (fork)", source.title),
            source.model.clone(),
            source.project_root.clone(),
            agent_name.to_string(),
            source.system_prompt.clone(),
            source.tools_schema.clone(),
            source.context_window,
        );
        new.messages = source.messages.clone();
        let new_id = new.id.clone();
        self.insert(new);
        Ok(new_id)
    }

    pub fn append_message(&self, id: &str, msg: Message) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        s.messages.push(msg);
        Ok(())
    }

    pub fn update_tokens(&self, id: &str, total: usize) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        s.total_tokens = Some(total);
        Ok(())
    }

    pub fn add_cost(&self, id: &str, cost: f64) -> anyhow::Result<()> {
        let mut sessions = self.sessions.lock().map_err(|e| anyhow::anyhow!("锁错误: {}", e))?;
        let s = sessions.get_mut(id).ok_or_else(|| anyhow::anyhow!("会话 {} 不存在", id))?;
        s.acc_cost += cost;
        Ok(())
    }
}

#[cfg(test)]
#[path = "store_test.rs"]
mod tests;
