use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt;
use rig_core::client::CompletionClient;
use rig_core::memory::ConversationMemory;
use rig_core::streaming::{StreamedAssistantContent, StreamingPrompt};
use rig_core::agent::MultiTurnStreamItem;
use tokio::sync::mpsc;

use crate::model::{StopReason, Usage};
use crate::session::message::{AssistantMessage, ContentBlock, Message};
use crate::session::session::Session;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AgentEvent {
    TextDelta(String),
    ReasoningDelta(String),
    ToolCallStart { id: String, name: String, arguments: String },
    ToolCallResult { id: String, result: String },
    Done { message: Message, usage: Usage, stop_reason: StopReason },
    Error(String),
}

pub struct StreamState {
    pub usage: Usage,
    pub text: String,
    model_id: String,
}

impl StreamState {
    pub fn new(model_id: String) -> Self {
        Self { usage: Usage::default(), text: String::new(), model_id }
    }

    pub fn handle<R>(&mut self, item: Result<MultiTurnStreamItem<R>, anyhow::Error>) -> Option<AgentEvent> {
        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(c)) => match c {
                StreamedAssistantContent::Text(t) => {
                    self.text.push_str(&t.text);
                    Some(AgentEvent::TextDelta(t.text))
                }
                StreamedAssistantContent::ToolCall { tool_call, .. } => {
                    Some(AgentEvent::ToolCallStart {
                        id: tool_call.id,
                        name: tool_call.function.name,
                        arguments: tool_call.function.arguments.to_string(),
                    })
                }
                _ => None,
            },
            Ok(MultiTurnStreamItem::StreamUserItem(u)) => match u {
                rig_core::streaming::StreamedUserContent::ToolResult { tool_result, .. } => {
                    let text = tool_result.content.iter()
                        .filter_map(|c| match c {
                            rig_core::completion::message::ToolResultContent::Text(t) => Some(t.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Some(AgentEvent::ToolCallResult { id: tool_result.id, result: text })
                }
            },
            Ok(MultiTurnStreamItem::CompletionCall(cc)) => {
                if let Some(u) = cc.usage {
                    self.usage.input += u.input_tokens as usize;
                    self.usage.output += u.output_tokens as usize;
                    self.usage.total += u.total_tokens as usize;
                }
                None
            }
            Ok(MultiTurnStreamItem::FinalResponse(r)) => {
                let ru = r.usage();
                self.usage.input += ru.input_tokens as usize;
                self.usage.output += ru.output_tokens as usize;
                self.usage.total += ru.total_tokens as usize;
                let u = self.usage.clone();
                let msg = Message::Assistant(AssistantMessage {
                    content: vec![ContentBlock::Text { text: std::mem::take(&mut self.text) }],
                    api: String::new(), provider: String::new(),
                    model: self.model_id.clone(),
                    usage: u.clone(), stop_reason: StopReason::Stop,
                    timestamp: 0,
                });
                Some(AgentEvent::Done { message: msg, usage: u, stop_reason: StopReason::Stop })
            }
            Err(e) => Some(AgentEvent::Error(e.to_string())),
            _ => None,
        }
    }
}

pub fn run_agent_stream(
    client: &rig_core::providers::openai::CompletionsClient,
    session: &Session,
    user_input: &str,
    tools: Vec<Box<dyn rig_core::tool::ToolDyn>>,
    memory: Option<Arc<dyn ConversationMemory>>,
    cancel_flag: Arc<AtomicBool>,
) -> mpsc::UnboundedReceiver<AgentEvent> {
    let (tx, rx) = mpsc::unbounded_channel();

    let mut builder = client
        .agent(&session.model.id)
        .preamble(&session.system_prompt)
        .tools(tools);
    if let Some(mem) = memory {
        builder = builder.memory(mem);
    }
    let agent = builder.build();

    let model_id = session.model.id.clone();
    let input = user_input.to_string();
    let cancel = cancel_flag.clone();

    tokio::spawn(async move {
        let mut stream = agent.stream_prompt(&input).multi_turn(20).await;
        let mut state = StreamState::new(model_id);

        while let Some(item) = stream.next().await {
            if cancel.load(Ordering::Relaxed) { break; }
            if let Some(event) = state.handle(item.map_err(|e| anyhow::anyhow!("{e}")))
                && tx.send(event).is_err() { break; }
        }
    });

    rx
}

#[cfg(test)]
#[path = "runner_test.rs"]
mod tests;
