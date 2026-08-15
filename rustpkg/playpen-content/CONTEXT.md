# 事件层（Content）

全系统共享的核心事件类型与消息内容块，零内部依赖。定义统一事件模型与 `ContentBlock` 格式化，供下游各层（配置、工具、持久化、运行时、交互）共享。

## 术语

**内容块（ContentBlock）**：
消息内容块，三种变体 `Text` / `Resource` / `ResourceLink`，serde 按 `type` 字段区分（`text` / `resource` / `resource_link`）。提供 `text()` / `resource()` / `resource_link()` 构造器与统一的 `with_annotations()`，也支持从 `String` / `&str` 转换。
_避免使用_：消息、消息体、payload

**文本内容（TextContent）**：
纯文本内容块。字段 `text` 与可选 `annotations`（JSON 元数据），提供 `new()` / `with_annotations()` 构造器。

**资源（Resource）**：
资源内容块，内联资源的文本或二进制数据。两种变体 `Text { uri, media_type, text }` 与 `Blob { uri, media_type, blob }`，其中 `blob` 以 base64 序列化。
_避免使用_：附件、文件

**资源链接（ResourceLink）**：
资源引用块，仅含 URI 引用而非内联数据。字段 `uri` / `name`，以及可选的 `media_type` / `size` / `annotations`。

**事件（Event）**：
统一事件模型，覆盖一次 agent 交互的完整生命周期。10 个变体：`UserMessage`、`ModelMessageDelta`、`ModelMessage`、`ModelThoughtDelta`、`ModelThought`、`FunctionCall`、`FunctionOutputDelta`、`FunctionResult`、`TurnStop`、`StateUpdate`。每个变体携带 `id`（store 分配的 event_id），提供 `event_id()` 读取、`with_id()` 赋值。

**结束原因（StopReason）**：
模型回合结束的原因。变体 `EndTurn` / `MaxTokens` / `MaxTurnRequests` / `Refusal` / `Cancelled` / `Error(String)`。

**Token 用量（TokenUsage）**：
单次回合的 token 用量统计。字段 `prompt_token_count` / `candidates_token_count` / `total_token_count`，以及可选的缓存读写与 thinking token 计数（`cache_read_input_token_count` / `cache_creation_input_token_count` / `thinking_token_count`）。

**内容块格式化（format_content_block）**：
将 `ContentBlock` 转为 LLM 可读文本。`Text` 输出纯文本；`Resource` 输出带 MIME 扩展名的代码块（`Text` 内联文本，`Blob` 转 base64），并标注 URI；`ResourceLink` 输出 Markdown 链接（可选标注 media_type）。
