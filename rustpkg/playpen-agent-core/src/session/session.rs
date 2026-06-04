use std::path::PathBuf;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::model::Model;
use crate::session::message::Message;
use crate::tool::ToolSchema;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub model: Model,
    pub project_root: PathBuf,
    pub agent_name: String,
    pub system_prompt: String,
    pub tools_schema: Vec<ToolSchema>,
    pub messages: Vec<Message>,
    pub acc_cost: f64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
    pub total_tokens: Option<usize>,
    pub context_window: Option<usize>,
}

impl Session {
    /// 上下文窗口使用率：total_tokens / context_window
    pub fn context_usage(&self) -> Option<f64> {
        match (self.total_tokens, self.context_window) {
            (Some(total), Some(window)) if window > 0 => Some(total as f64 / window as f64),
            _ => None,
        }
    }

    /// 上下文是否接近限制（使用率 > 0.9）
    pub fn is_context_near_limit(&self) -> bool {
        self.context_usage().is_some_and(|usage| usage > 0.9)
    }
}

pub fn create_session(
    title: String,
    model: Model,
    project_root: PathBuf,
    agent_name: String,
    system_prompt: String,
    tools_schema: Vec<ToolSchema>,
    context_window: Option<usize>,
) -> Session {
    Session {
        id: Uuid::new_v4().to_string(),
        title,
        model,
        project_root,
        agent_name,
        system_prompt,
        tools_schema,
        messages: Vec::new(),
        acc_cost: 0.0,
        currency: "USD".to_string(),
        created_at: Utc::now(),
        archived_at: None,
        total_tokens: None,
        context_window,
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
