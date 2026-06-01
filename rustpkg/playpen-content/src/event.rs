use crate::content::ContentBlock;

impl Event {
    /// 获取 store 分配的 event_id。
    pub fn event_id(&self) -> Option<&str> {
        match self {
            Event::UserMessage { id, .. }
            | Event::ModelMessage { id, .. }
            | Event::ModelThought { id, .. }
            | Event::FunctionCall { id, .. }
            | Event::FunctionOutputDelta { id, .. }
            | Event::FunctionResult { id, .. }
            | Event::TurnStop { id, .. }
            | Event::ModelMessageDelta { id, .. }
            | Event::ModelThoughtDelta { id, .. } => Some(id.as_str()),
            Event::StateUpdate { id, .. } => Some(id.as_str()),
        }
    }

    /// 为事件赋予 store 分配的 event_id，返回自身。
    pub fn with_id(self, id: String) -> Self {
        match self {
            Event::UserMessage { content, .. } => Event::UserMessage { id, content },
            Event::ModelMessage { content, .. } => Event::ModelMessage { id, content },
            Event::ModelThought { text, .. } => Event::ModelThought { id, text },
            Event::FunctionCall {
                call_id,
                name,
                args,
                ..
            } => Event::FunctionCall {
                id,
                call_id,
                name,
                args,
            },
            Event::FunctionResult {
                call_id,
                name,
                content,
                code,
                ..
            } => Event::FunctionResult {
                id,
                call_id,
                name,
                content,
                code,
            },
            Event::FunctionOutputDelta {
                call_id,
                name,
                text,
                ..
            } => Event::FunctionOutputDelta {
                id,
                call_id,
                name,
                text,
            },
            Event::TurnStop {
                stop_reason,
                token_usage,
                ..
            } => Event::TurnStop {
                id,
                stop_reason,
                token_usage,
            },
            Event::StateUpdate { name, data, .. } => Event::StateUpdate { id, name, data },
            Event::ModelMessageDelta { text, .. } => Event::ModelMessageDelta { id, text },
            Event::ModelThoughtDelta { text, .. } => Event::ModelThoughtDelta { id, text },
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Event {
    // ── User ──
    UserMessage {
        id: String,
        content: Vec<ContentBlock>,
    },

    // ── Model message ──
    ModelMessageDelta {
        id: String,
        text: String,
    },

    ModelMessage {
        id: String,
        content: Vec<ContentBlock>,
    },

    // ── Model thought ──
    ModelThoughtDelta {
        id: String,
        text: String,
    },

    ModelThought {
        id: String,
        text: String,
    },

    // ── Function ──
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        args: serde_json::Value,
    },

    FunctionOutputDelta {
        id: String,
        call_id: String,
        name: String,
        text: String,
    },

    FunctionResult {
        id: String,
        call_id: String,
        name: String,
        content: Option<Vec<ContentBlock>>,
        code: Option<i32>,
    },
    // ── Turn ──
    TurnStop {
        id: String,
        stop_reason: StopReason,
        token_usage: Option<TokenUsage>,
    },
    // ── State update ──
    StateUpdate {
        id: String,
        name: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenUsage {
    pub prompt_token_count: i32,
    pub candidates_token_count: i32,
    pub total_token_count: i32,
    pub cache_read_input_token_count: Option<i32>,
    pub cache_creation_input_token_count: Option<i32>,
    pub thinking_token_count: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Cancelled,
    Error(String),
}

#[cfg(test)]
#[path = "event_test.rs"]
mod tests;
