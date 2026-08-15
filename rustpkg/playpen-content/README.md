# playpen-content

事件层核心类型，零内部依赖。定义统一事件模型与消息内容块，供 `playpen-agent` 等下游 crate 共享。

## 职责

- `ContentBlock`：消息内容块，三种变体 `Text` / `Resource` / `ResourceLink`，支持统一附加 annotations
- `format_content_block()`：将 `ContentBlock` 格式化为 LLM 可见文本
- `Event`：统一事件模型（`UserMessage` / `ModelMessage` / `ModelThought` / `FunctionCall` / `FunctionResult` / `TurnStop` / `StateUpdate` 等）
- `StopReason` / `TokenUsage`：结束原因与 token 用量统计

## 依赖

零内部依赖，仅 `serde` / `serde_json` / `base64` / `mime_guess`。
