//! Event ↔ rig Message 转换。
//!
//! 用于将 session 中的 Event 历史转换为 rig 的聊天消息序列。
//! 以及将 rig streaming response 的 chunk 流转换为 Event 流。

use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use playpen_content::{ContentBlock, Event, format_content_block};
use rig_core::completion::GetTokenUsage;
use rig_core::completion::Message;
use rig_core::completion::message::{
    AssistantContent, Reasoning, Text, ToolCall, ToolFunction, ToolResult, ToolResultContent,
    UserContent,
};
use rig_core::one_or_many::OneOrMany;
use rig_core::streaming::StreamedAssistantContent;
use uuid::Uuid;

/// 将 Event 转换为 rig AssistantContent（Text / Reasoning / ToolCall）。
/// 用于在 events_to_chat_history 中累积为单个 Message::Assistant。
pub fn event_to_assistant_content(event: &Event) -> Option<AssistantContent> {
    match event {
        Event::ModelMessage { content, .. } => {
            let text: String = content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect();
            if text.is_empty() {
                return None;
            }
            Some(AssistantContent::Text(Text {
                text,
                additional_params: None,
            }))
        }
        Event::ModelThought { text, .. } => Some(AssistantContent::Reasoning(Reasoning::new(text))),
        Event::FunctionCall {
            call_id: id,
            name,
            args,
            ..
        } => Some(AssistantContent::ToolCall(ToolCall {
            id: id.clone(),
            call_id: None,
            function: ToolFunction::new(name.clone(), args.clone()),
            signature: None,
            additional_params: None,
        })),
        _ => None,
    }
}

/// 将 UserMessage Event 转换为 Message::User（保留所有 ContentBlock 类型）。
pub fn event_to_user_message(event: &Event) -> Option<Message> {
    match event {
        Event::UserMessage { content, .. } => {
            let blocks = content_blocks_to_user_content(content);
            if blocks.is_empty() {
                return None;
            }
            Some(Message::User {
                content: OneOrMany::many(blocks).unwrap(),
            })
        }
        _ => None,
    }
}

/// 将 FunctionResult Event 转换为 Message::User（ToolResult）。
/// 保留 ContentBlock 中所有文本类型。
pub fn event_to_tool_result(event: &Event) -> Option<Message> {
    match event {
        Event::FunctionResult {
            call_id,
            content,
            code,
            ..
        } => {
            let text: String = content
                .as_ref()
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text(t) => Some(t.text.clone()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();

            let mut result_json = serde_json::Map::new();
            result_json.insert("result".into(), text.into());
            if let Some(c) = code {
                result_json.insert("exit_code".into(), (*c).into());
            }

            Some(Message::User {
                content: OneOrMany::one(UserContent::ToolResult(ToolResult {
                    id: call_id.clone(),
                    call_id: None,
                    content: OneOrMany::one(ToolResultContent::Text(Text {
                        text: serde_json::to_string(&serde_json::Value::Object(result_json))
                            .unwrap_or_default(),
                        additional_params: None,
                    })),
                })),
            })
        }
        _ => None,
    }
}

/// 将 ContentBlock 切片转换为 Vec<UserContent>（保留所有类型）。
fn content_blocks_to_user_content(blocks: &[ContentBlock]) -> Vec<UserContent> {
    blocks
        .iter()
        .map(|b| {
            UserContent::Text(Text {
                text: format_content_block(b),
                additional_params: None,
            })
        })
        .collect()
}

/// 将 Session 事件流转换为 rig 聊天消息序列。
///
/// 同一 turn 内的连续 ModelMessage / ModelThought / FunctionCall
/// 合并为单个 Message::Assistant，UserMessage / FunctionResult / 流结束触发刷新。
pub fn events_to_chat_history<'a>(
    events: impl Stream<Item = Event> + Send + Unpin + 'a,
) -> BoxStream<'a, Message> {
    use std::collections::VecDeque;
    use std::mem;

    struct State<S> {
        stream: S,
        /// 待出队的 Message
        queue: VecDeque<Message>,
        /// 累积的 AssistantContent
        acc: Vec<AssistantContent>,
    }

    fn flush(acc: &mut Vec<AssistantContent>) -> Option<Message> {
        if acc.is_empty() {
            return None;
        }
        let contents = mem::take(acc);
        Some(Message::Assistant {
            id: None,
            content: OneOrMany::many(contents).unwrap(),
        })
    }

    fn push_event(event: Event, queue: &mut VecDeque<Message>, acc: &mut Vec<AssistantContent>) {
        match event {
            // 用户消息：先刷新累积的 assistant，再排队 user message
            Event::UserMessage { .. } => {
                if let Some(msg) = flush(acc) {
                    queue.push_back(msg);
                }
                if let Some(msg) = event_to_user_message(&event) {
                    queue.push_back(msg);
                }
            }
            // 工具结果：先刷新累积的 assistant，再排队 tool result
            Event::FunctionResult { .. } => {
                if let Some(msg) = flush(acc) {
                    queue.push_back(msg);
                }
                if let Some(msg) = event_to_tool_result(&event) {
                    queue.push_back(msg);
                }
            }
            // assistant 事件：累积到 acc
            _ => {
                if let Some(content) = event_to_assistant_content(&event) {
                    acc.push(content);
                }
            }
        }
    }

    futures::stream::unfold(
        State {
            stream: events,
            queue: VecDeque::new(),
            acc: Vec::new(),
        },
        |mut state| async move {
            // 优先出队
            if let Some(msg) = state.queue.pop_front() {
                return Some((msg, state));
            }

            // 消费 stream，将事件压入队列
            while let Some(event) = state.stream.next().await {
                push_event(event, &mut state.queue, &mut state.acc);
                if let Some(msg) = state.queue.pop_front() {
                    return Some((msg, state));
                }
            }

            // stream 耗尽，刷新剩余助理内容
            flush(&mut state.acc).map(|msg| (msg, state))
        },
    )
    .boxed()
}

/// 为 Stream 添加 `.pipe()` 方法，用于函数组合。
pub trait StreamPipe: Stream + Sized {
    fn pipe<B>(self, f: impl FnOnce(Self) -> B) -> B {
        f(self)
    }
}

impl<S: Stream + Sized> StreamPipe for S {}

/// `Final` chunk 的解析结果。
pub struct FinalResponseInfo {
    /// token 用量。
    pub token_usage: Option<playpen_content::TokenUsage>,
    /// completion API 返回的 finish_reason（如 `"stop"` / `"length"` / `"refusal"`）。
    /// 仅对已知的 Response 类型（deepseek / openai）有值。
    pub finish_reason: Option<String>,
}

/// 将 completion API 的 finish_reason 映射为 playpen 的 StopReason。
pub fn finish_reason_to_stop_reason(fr: Option<&str>) -> playpen_content::StopReason {
    match fr {
        Some("stop") | None => playpen_content::StopReason::EndTurn,
        Some("length" | "max_tokens") => playpen_content::StopReason::MaxTokens,
        Some("refusal" | "content_filter") => playpen_content::StopReason::Refusal,
        // "tool_calls" 由 runner 根据上下文决定
        Some(_) => playpen_content::StopReason::EndTurn,
    }
}

// ── Streaming 转换 ──────────────────────────────────────────────────────

/// 将 rig streaming response 的 chunk 流转换为 Event 惰性迭代器。
///
/// 产出规则：
/// - `ModelMessageDelta` / `ModelThoughtDelta` —— 实时增量，每个 chunk
/// - `ModelThought` / `ModelMessage` —— tool_call 前或流结束时，累积内容的完整记录
/// - `FunctionCall` —— 遇到 tool_call 时
/// - `TurnStop` —— 总是产出，由 runner 决定是否持久化
pub fn process_stream<S, E, R>(
    stream: S,
    extract_finish_reason: fn(&dyn std::any::Any) -> Option<String>,
) -> impl Stream<Item = Event>
where
    S: Stream<Item = Result<StreamedAssistantContent<R>, E>> + Unpin,
    E: std::fmt::Display,
    R: GetTokenUsage + 'static,
{
    use std::collections::VecDeque;

    /// 累积的文本内容及其首段分配的 id。
    /// - `ensure_id()`: id 为空时自动分配新 id
    /// - `take()`: 取出 (id, text) 并重置，下次使用自动分配新 id
    struct AccumulatedText {
        id: String,
        text: String,
    }

    impl AccumulatedText {
        fn ensure_id(&mut self) {
            if self.id.is_empty() {
                self.id = next_id();
            }
        }
        fn is_empty(&self) -> bool {
            self.text.is_empty()
        }
        fn push_str(&mut self, s: &str) {
            self.text.push_str(s);
        }
        /// 取出 (id, text) 并重置，下次使用自动分配新 id
        fn take(&mut self) -> (String, String) {
            let id = std::mem::take(&mut self.id);
            let text = std::mem::take(&mut self.text);
            (id, text)
        }
    }

    fn next_id() -> String {
        Uuid::now_v7().to_string()
    }

    struct State<S> {
        stream: S,
        /// 累积的文本内容（tool_call 前或流结束时刷出完整的 ModelMessage）
        full_text: AccumulatedText,
        /// 累积的推理内容（tool_call 前或流结束时刷出完整的 ModelThought）
        reasoning_text: AccumulatedText,
        /// 待产出的事件队列
        pending: VecDeque<Event>,
        /// stream 已耗尽，收尾事件已入队或已全部产出
        done: bool,
    }

    let info = std::sync::Arc::new(std::sync::Mutex::new(FinalResponseInfo {
        token_usage: None,
        finish_reason: None,
    }));

    let info_clone = info.clone();

    futures::stream::unfold(
        State {
            stream,
            full_text: AccumulatedText {
                id: String::new(),
                text: String::new(),
            },
            reasoning_text: AccumulatedText {
                id: String::new(),
                text: String::new(),
            },
            pending: VecDeque::new(),
            done: false,
        },
        move |mut state| {
            let info = info_clone.clone();
            let extract = extract_finish_reason;
            async move {
                // 1. 优先从 pending 队列吐出
                if let Some(event) = state.pending.pop_front() {
                    return Some((event, state));
                }

                // 2. stream 已耗尽且没有 pending 事件
                if state.done {
                    return None;
                }

                // 3. 消费 stream chunk
                while let Some(chunk) = state.stream.next().await {
                    match chunk {
                        Ok(StreamedAssistantContent::Text(text)) => {
                            state.full_text.ensure_id();
                            state.full_text.push_str(&text.text);
                            return Some((
                                Event::ModelMessageDelta {
                                    id: state.full_text.id.clone(),
                                    text: text.text,
                                },
                                state,
                            ));
                        }
                        Ok(StreamedAssistantContent::Reasoning(_)) => {
                            // 当前所有 provider 仅输出 ReasoningDelta，完整 reasoning 块暂不处理。
                            continue;
                        }
                        Ok(StreamedAssistantContent::ReasoningDelta { reasoning, .. }) => {
                            state.reasoning_text.ensure_id();
                            state.reasoning_text.push_str(&reasoning);
                            return Some((
                                Event::ModelThoughtDelta {
                                    id: state.reasoning_text.id.clone(),
                                    text: reasoning,
                                },
                                state,
                            ));
                        }
                        Ok(StreamedAssistantContent::ToolCall { tool_call, .. }) => {
                            let call = Event::FunctionCall {
                                id: next_id(),
                                call_id: tool_call.id,
                                name: tool_call.function.name,
                                args: tool_call.function.arguments,
                            };

                            // 有累积内容 → 按 [thought, text, call] 顺序入 pending
                            let has_reasoning = !state.reasoning_text.is_empty();
                            let has_text = !state.full_text.is_empty();

                            if has_reasoning || has_text {
                                state.pending.push_back(call);
                                if has_text {
                                    let (id, text) = state.full_text.take();
                                    state.pending.push_front(Event::ModelMessage {
                                        id,
                                        content: vec![ContentBlock::text(text)],
                                    });
                                }
                                if has_reasoning {
                                    let (id, text) = state.reasoning_text.take();
                                    state.pending.push_front(Event::ModelThought { id, text });
                                }
                                return Some((state.pending.pop_front().unwrap(), state));
                            }

                            // 无累积直接 yield
                            return Some((call, state));
                        }
                        Ok(StreamedAssistantContent::ToolCallDelta { .. }) => {}
                        Ok(StreamedAssistantContent::Final(response)) => {
                            let mut guard = info.lock().unwrap();
                            guard.finish_reason = extract(&response);
                            if let Some(u) = extract_token_usage(&response) {
                                guard.token_usage = Some(u);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "stream chunk error, skipping"
                            );
                        }
                    }
                }

                // 4. 流耗尽 — 按 [thought, text, turn_stop] 顺序入 pending
                state.done = true;
                {
                    let guard = info.lock().unwrap();
                    let stop_reason = finish_reason_to_stop_reason(guard.finish_reason.as_deref());
                    state.pending.push_back(Event::TurnStop {
                        id: next_id(),
                        stop_reason,
                        token_usage: guard.token_usage.clone(),
                    });
                }
                if !state.full_text.is_empty() {
                    let (id, text) = state.full_text.take();
                    state.pending.push_front(Event::ModelMessage {
                        id,
                        content: vec![ContentBlock::text(text)],
                    });
                }
                if !state.reasoning_text.is_empty() {
                    let (id, text) = state.reasoning_text.take();
                    state.pending.push_front(Event::ModelThought { id, text });
                }

                state.pending.pop_front().map(|event| (event, state))
            }
        },
    )
}

/// 从 rig streaming response 中提取 TokenUsage。
pub fn extract_token_usage(r: &impl GetTokenUsage) -> Option<playpen_content::TokenUsage> {
    let usage = r.token_usage();
    (usage.output_tokens > 0 || usage.input_tokens > 0).then(|| playpen_content::TokenUsage {
        prompt_token_count: usage.input_tokens as i32,
        candidates_token_count: usage.output_tokens as i32,
        total_token_count: usage.total_tokens as i32,
        cache_read_input_token_count: Some(usage.cached_input_tokens as i32).filter(|v| *v > 0),
        cache_creation_input_token_count: Some(usage.cache_creation_input_tokens as i32)
            .filter(|v| *v > 0),
        thinking_token_count: None,
    })
}

#[cfg(test)]
#[path = "convert_test.rs"]
mod tests;
