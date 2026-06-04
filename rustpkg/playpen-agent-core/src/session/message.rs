use serde::{Deserialize, Serialize};

use crate::model::{StopReason, Usage};
use crate::tool::ToolSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Thinking { thinking: String },
    ToolCall { id: String, name: String, arguments: serde_json::Value },
    Image { data: String, mime_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "system")]
    System(SystemMessage),
    #[serde(rename = "user")]
    User(UserMessage),
    #[serde(rename = "assistant")]
    Assistant(AssistantMessage),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultMessage),
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message::System(SystemMessage {
            content: content.into(),
            tools: None,
            timestamp: 0,
        })
    }

    pub fn user(content: impl Into<String>) -> Self {
        Message::User(UserMessage {
            content: Some(content.into()),
            images: Vec::new(),
            timestamp: 0,
        })
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Message::Assistant(AssistantMessage {
            content: vec![ContentBlock::Text { text: content.into() }],
            api: String::new(),
            provider: String::new(),
            model: String::new(),
            usage: Usage::default(),
            stop_reason: StopReason::Stop,
            timestamp: 0,
        })
    }

    pub fn tool(tool_call_id: impl Into<String>, result: impl Into<String>) -> Self {
        Message::ToolResult(ToolResultMessage {
            tool_call_id: tool_call_id.into(),
            tool_name: String::new(),
            content: vec![ContentBlock::Text { text: result.into() }],
            is_error: false,
            timestamp: 0,
        })
    }

    /// 获取纯文本内容
    pub fn text_content(&self) -> Option<&str> {
        match self {
            Message::System(msg) => Some(msg.content.as_str()),
            Message::User(msg) => msg.content.as_deref(),
            Message::Assistant(msg) => {
                if let Some(ContentBlock::Text { text }) = msg.content.first() {
                    Some(text.as_str())
                } else {
                    None
                }
            }
            Message::ToolResult(msg) => {
                if let Some(ContentBlock::Text { text }) = msg.content.first() {
                    Some(text.as_str())
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMessage {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSchema>>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub images: Vec<ImageContent>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    pub content_type: String,
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    pub api: String,
    pub provider: String,
    pub model: String,
    pub usage: Usage,
    pub stop_reason: StopReason,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub tool_name: String,
    pub content: Vec<ContentBlock>,
    pub is_error: bool,
    pub timestamp: i64,
}

#[cfg(test)]
#[path = "message_test.rs"]
mod tests;
