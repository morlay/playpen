//! Event ↔ rig Message 转换。
//!
//! 用于将 session 中的 Event 历史转换为 rig 的聊天消息序列。
//! 以及将 rig streaming response 的 chunk 流转换为 Event 流。
//!
//! 图片类 ContentBlock 通过 [`ImageUploader`] 上传到 provider files API，
//! 以 `file_id` 引用发送（DeepSeek Files API 的 `{"type":"file","file_id":...}` 块）。

use futures::Stream;
use futures::StreamExt;
use futures::stream::BoxStream;
use playpen_content::{
    ContentBlock, Event, Resource, StopReason, TokenUsage, format_content_block,
};
use rig_core::completion::FinishReason;
use rig_core::completion::Message;
use rig_core::completion::message::{
    AssistantContent, Document, DocumentSourceKind, Reasoning, Text, ToolCall, ToolFunction,
    ToolResult, ToolResultContent, UserContent,
};
use rig_core::streaming::StreamedAssistantContent;
use std::sync::Arc;
use uuid::Uuid;

// ── 图片上传 ────────────────────────────────────────────────────────────

/// 图片上传器：将图片数据上传到 provider files API，返回 `file_id`。
///
/// 由 runner 构造并注入转换层；`None`（未注入）时图片维持文本化兜底。
#[async_trait::async_trait]
pub trait ImageUploader: Send + Sync {
    /// 上传图片数据，返回 provider 侧 `file_id`。实现应自带去重缓存。
    async fn upload_image(
        &self,
        name: &str,
        media_type: &str,
        data: Vec<u8>,
    ) -> anyhow::Result<String>;

    /// 读取本地图片文件（`ResourceLink` 场景），`uri` 可为 `file://` 或相对/绝对路径。
    async fn read_image_file(&self, uri: &str) -> anyhow::Result<Vec<u8>>;
}

/// 判断 ContentBlock 是否为可上传的图片类内容。
pub fn is_image_block(block: &ContentBlock) -> bool {
    match block {
        ContentBlock::Resource(Resource::Blob { media_type, .. }) => {
            is_image_media_type(media_type)
        }
        ContentBlock::Resource(Resource::Text { .. }) => false,
        ContentBlock::ResourceLink(link) => match &link.media_type {
            Some(mt) => is_image_media_type(mt),
            None => is_image_uri(&link.uri),
        },
        ContentBlock::Text(_) => false,
    }
}

fn is_image_media_type(mt: &str) -> bool {
    mt.starts_with("image/")
}

fn is_image_uri(uri: &str) -> bool {
    let ext = std::path::Path::new(uri)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "gif" | "webp")
}

/// 从 uri 推断文件名，缺省时按 media_type 生成扩展名。
fn file_name_from(uri: &str, media_type: &str) -> String {
    let name = std::path::Path::new(uri)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty() || name == "/" {
        let ext = media_type
            .strip_prefix("image/")
            .map(|s| s.replace("jpeg", "jpg"))
            .unwrap_or_else(|| "img".into());
        format!("image.{ext}")
    } else {
        name
    }
}

/// 将图片 ContentBlock 解析为 (文件名, media_type, 字节数据)。
/// `ResourceLink` 需要读取本地文件，读取失败返回 `None`（调用方文本化兜底）。
async fn image_payload(
    block: &ContentBlock,
    uploader: &dyn ImageUploader,
) -> Option<(String, String, Vec<u8>)> {
    match block {
        ContentBlock::Resource(Resource::Blob {
            uri,
            media_type,
            blob,
            ..
        }) => Some((
            file_name_from(uri, media_type),
            media_type.clone(),
            blob.clone(),
        )),
        ContentBlock::ResourceLink(link) => {
            let data = uploader.read_image_file(&link.uri).await.ok()?;
            let media_type = link
                .media_type
                .clone()
                .unwrap_or_else(|| media_type_from_uri(&link.uri));
            Some((file_name_from(&link.uri, &media_type), media_type, data))
        }
        _ => None,
    }
}

/// 按扩展名推断图片 MIME（仅支持 Files API 的 JPEG/PNG/GIF/WebP）。
fn media_type_from_uri(uri: &str) -> String {
    let ext = std::path::Path::new(uri)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// 生成 DeepSeek files API 的 file 引用块（rig 侧以 Document::FileId 承载，
/// provider 序列化后改写为 `{"type":"file","file_id":...}`）。
fn file_ref_content(file_id: String) -> UserContent {
    UserContent::Document(Document {
        data: DocumentSourceKind::FileId(file_id),
        media_type: None,
        additional_params: None,
    })
}

// ── Event → rig Message ────────────────────────────────────────────────

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
        } => Some(AssistantContent::ToolCall(ToolCall::new(
            rig_core::completion::message::ToolCallId::new_or_mint(id.clone()),
            ToolFunction::new(name.clone(), args.clone()),
        ))),
        _ => None,
    }
}

/// 将 UserMessage Event 转换为 Message::User。
/// 注入 [`ImageUploader`] 时，图片类 ContentBlock 先上传再以 file 引用块发送。
pub async fn event_to_user_message(
    event: &Event,
    uploader: Option<&Arc<dyn ImageUploader>>,
) -> Option<Message> {
    match event {
        Event::UserMessage { content, .. } => {
            let blocks = content_blocks_to_user_content(content, uploader).await;
            if blocks.is_empty() {
                return None;
            }
            Some(Message::User { content: blocks })
        }
        _ => None,
    }
}

/// 将 FunctionResult Event 转换为 Message::User（ToolResult）。
/// 保留 ContentBlock 中所有文本类型；二进制 Blob 不进 LLM 文本（避免 base64 爆量）。
pub fn event_to_tool_result(event: &Event) -> Option<Message> {
    match event {
        Event::FunctionResult {
            call_id,
            name,
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
                            // 二进制内容不序列化进 tool result 文本
                            ContentBlock::Resource(Resource::Blob { .. }) => None,
                            other => Some(format_content_block(other)),
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
                content: vec![UserContent::ToolResult(ToolResult {
                    call: rig_core::completion::message::ToolCallId::new_or_mint(call_id.clone()),
                    provider: None,
                    name: name.clone(),
                    content: vec![ToolResultContent::Text(Text {
                        text: serde_json::to_string(&serde_json::Value::Object(result_json))
                            .unwrap_or_default(),
                        additional_params: None,
                    })],
                })],
            })
        }
        _ => None,
    }
}

/// 将 ContentBlock 切片转换为 Vec<UserContent>。
/// 图片块（注入 uploader 时）→ file 引用块；其余 → 文本化。
async fn content_blocks_to_user_content(
    blocks: &[ContentBlock],
    uploader: Option<&Arc<dyn ImageUploader>>,
) -> Vec<UserContent> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let uploaded = match uploader {
            Some(uploader) if is_image_block(block) => {
                if let Some((name, media_type, data)) =
                    image_payload(block, uploader.as_ref()).await
                {
                    match uploader.upload_image(&name, &media_type, data).await {
                        Ok(file_id) => Some(file_ref_content(file_id)),
                        Err(e) => {
                            tracing::warn!(error = %e, name, "image upload failed, fallback to text");
                            None
                        }
                    }
                } else {
                    None
                }
            }
            _ => None,
        };
        out.push(uploaded.unwrap_or_else(|| {
            UserContent::Text(Text {
                text: format_content_block(block),
                additional_params: None,
            })
        }));
    }
    out
}

/// 将 Session 事件流转换为 rig 聊天消息序列。
///
/// 同一 turn 内的连续 ModelMessage / ModelThought / FunctionCall
/// 合并为单个 Message::Assistant，UserMessage / FunctionResult / 流结束触发刷新。
/// 注入 [`ImageUploader`] 时图片类 UserMessage 会先上传再发送。
pub fn events_to_chat_history<'a>(
    events: impl Stream<Item = Event> + Send + Unpin + 'a,
    uploader: Option<Arc<dyn ImageUploader>>,
) -> BoxStream<'a, Message> {
    use std::collections::HashSet;
    use std::collections::VecDeque;
    use std::mem;

    struct State<S> {
        stream: S,
        /// 待出队的 Message
        queue: VecDeque<Message>,
        /// 累积的 AssistantContent
        acc: Vec<AssistantContent>,
        /// 已出现的 FunctionCall 的 call_id，用于配对 FunctionResult
        call_ids: HashSet<String>,
    }

    fn flush(acc: &mut Vec<AssistantContent>) -> Option<Message> {
        if acc.is_empty() {
            return None;
        }
        let contents = mem::take(acc);
        Some(Message::Assistant {
            id: None,
            content: contents,
        })
    }

    async fn push_event(
        event: Event,
        queue: &mut VecDeque<Message>,
        acc: &mut Vec<AssistantContent>,
        call_ids: &mut HashSet<String>,
        uploader: &Option<Arc<dyn ImageUploader>>,
    ) {
        match &event {
            // 用户消息：先刷新累积的 assistant，再排队 user message
            Event::UserMessage { .. } => {
                if let Some(msg) = flush(acc) {
                    queue.push_back(msg);
                }
                if let Some(msg) = event_to_user_message(&event, uploader.as_ref()).await {
                    queue.push_back(msg);
                }
            }
            // 工具结果：先刷新累积的 assistant，再排队 tool result。
            // 请求兜底：孤儿 FunctionResult（call_id 无对应 FunctionCall）直接丢弃，
            // 不传给 LLM——否则 LLM 会收到一个没有 tool_call 的 tool_result 而报错。
            Event::FunctionResult { call_id, .. } => {
                if !call_ids.contains(call_id) {
                    tracing::warn!(call_id, "丢弃无对应 FunctionCall 的 FunctionResult");
                    return;
                }
                if let Some(msg) = flush(acc) {
                    queue.push_back(msg);
                }
                if let Some(msg) = event_to_tool_result(&event) {
                    queue.push_back(msg);
                }
            }
            // FunctionCall：记录 call_id 供 FunctionResult 配对，并累积为 ToolCall
            Event::FunctionCall { call_id, .. } => {
                call_ids.insert(call_id.clone());
                if let Some(content) = event_to_assistant_content(&event) {
                    acc.push(content);
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

    let uploader_for_unfold = uploader.clone();
    futures::stream::unfold(
        State {
            stream: events,
            queue: VecDeque::new(),
            acc: Vec::new(),
            call_ids: HashSet::new(),
        },
        move |mut state| {
            let uploader = uploader_for_unfold.clone();
            async move {
                // 优先出队
                if let Some(msg) = state.queue.pop_front() {
                    return Some((msg, state));
                }

                // 消费 stream，将事件压入队列
                while let Some(event) = state.stream.next().await {
                    push_event(
                        event,
                        &mut state.queue,
                        &mut state.acc,
                        &mut state.call_ids,
                        &uploader,
                    )
                    .await;
                    if let Some(msg) = state.queue.pop_front() {
                        return Some((msg, state));
                    }
                }

                // stream 耗尽，刷新剩余助理内容
                flush(&mut state.acc).map(|msg| (msg, state))
            }
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
    pub token_usage: Option<TokenUsage>,
    /// completion API 返回的 finish_reason（rig 标准化枚举）。
    pub finish_reason: Option<FinishReason>,
}

/// 将 rig 标准化的 finish_reason 映射为 playpen 的 StopReason。
pub fn finish_reason_to_stop_reason(fr: Option<&FinishReason>) -> StopReason {
    match fr {
        None | Some(FinishReason::Stop) => StopReason::EndTurn,
        Some(FinishReason::Length) => StopReason::MaxTokens,
        Some(FinishReason::ContentFilter) => StopReason::Refusal,
        // "tool_calls" 由 runner 根据上下文决定
        Some(FinishReason::ToolCalls) | Some(FinishReason::Other(_)) => StopReason::EndTurn,
    }
}

/// 将 rig 标准化 Usage 转换为 playpen TokenUsage。
pub fn usage_to_playpen(usage: &rig_core::completion::Usage) -> Option<TokenUsage> {
    (usage.output_tokens > 0 || usage.input_tokens > 0).then(|| TokenUsage {
        prompt_token_count: usage.input_tokens as i32,
        candidates_token_count: usage.output_tokens as i32,
        total_token_count: usage.total_tokens as i32,
        cache_read_input_token_count: Some(usage.cached_input_tokens as i32).filter(|v| *v > 0),
        cache_creation_input_token_count: Some(usage.cache_creation_input_tokens as i32)
            .filter(|v| *v > 0),
        thinking_token_count: Some(usage.reasoning_tokens as i32).filter(|v| *v > 0),
    })
}

// ── Streaming 转换 ──────────────────────────────────────────────────────

/// 将 rig streaming response 的 chunk 流转换为 Event 惰性迭代器。
///
/// 产出规则：
/// - `ModelMessageDelta` / `ModelThoughtDelta` —— 实时增量，每个 chunk
/// - `ModelThought` / `ModelMessage` —— tool_call 前或流结束时，累积内容的完整记录
/// - `FunctionCall` —— 遇到 tool_call 时
/// - `TurnStop` —— 总是产出，由 runner 决定是否持久化
pub fn process_stream<S, E>(stream: S) -> impl Stream<Item = Event>
where
    S: Stream<Item = Result<StreamedAssistantContent, E>> + Unpin,
    E: std::fmt::Display,
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
                        Ok(StreamedAssistantContent::Reasoning { .. }) => {
                            // 当前所有 provider 仅输出 ReasoningDelta，完整 reasoning 块暂不处理。
                            continue;
                        }
                        Ok(StreamedAssistantContent::Unknown(_)) => {
                            // provider 特有的未知流式字段（如 assistant_items），跳过。
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
                                call_id: tool_call.id.as_str().to_string(),
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
                        Ok(StreamedAssistantContent::Final(final_response)) => {
                            let mut guard = info.lock().unwrap();
                            guard.finish_reason = final_response.finish_reason;
                            guard.token_usage = usage_to_playpen(&final_response.usage);
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
                    let stop_reason = finish_reason_to_stop_reason(guard.finish_reason.as_ref());
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

#[cfg(test)]
#[path = "convert_test.rs"]
mod tests;
