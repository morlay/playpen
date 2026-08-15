# Agent 运行时

运行时层核心：把 profile、session、LLM 与工具串联成一次 Agent 执行，提供执行视图、工厂、子代理与工具抽象。

## 术语

**执行视图（AgentRunner）**：
一次 session 的执行视图（trait）。访问器：`id()` / `session()` / `profile()` / `settings()`；`with_profile(p)` 与 `with_subagent_host(builder)` 返回注入新配置或子代理宿主的新 runner（不修改 self）。核心方法：`run(prompt: Vec<ContentBlock>)` 返回 `Event` 流；`rewind()` 回退到最近一条 user message 之前；`replay()` 重放 session 全部事件；`cancel()` 取消运行。
_避免使用_：执行器、运行器

**Runner 工厂（AgentRunnerBuilder）**：
runner 工厂（trait）。`create(profile)` 新建 session 并返回 runner；`resume(id)` 恢复既有 session；`agent_profiles()` 列出可用 profile；`sessions()` 返回底层 `SessionService`。
_避免使用_：runner 仓库、构建器

**生产实现（SimpleRunner / SimpleRunnerBuilder）**：
`AgentRunner` / `AgentRunnerBuilder` 基于 `playpen-session` 的实现。`SimpleRunnerBuilder::new(settings, dirs, session_service, profile_resolver)` 构造；`SimpleRunner` 持有 session、profile、settings、`CancellationToken` 与可选 `subagent_host`。另提供具体类型版本 `with_profile_typed` / `with_subagent_host_typed` 供包装类型复用。
_避免使用_：默认 runner、简单 runner

**LLM 客户端（LlmClient / LlmConfig / ModelEnum）**：
rig-core 之上的 LLM 客户端。`LlmConfig::from_settings(settings, profile)` 解析出 base_url / api_key / model / model_config；`is_deepseek_compat()` 按模型名前缀（deepseek / glm / mimo）判断协议；`ModelEnum` 枚举 `Deepseek` / `Openai` 两种兼容协议，各携带 finish_reason 提取器；`LlmClient::build_model()` 统一返回 `ModelEnum`。
_避免使用_：模型客户端、模型配置

**子代理（SubagentHost / SubagentHandle / SubagentOutput / RunnerSubagentHost）**：
子代理能力。`SubagentHost::spawn(label, session_id)` 合并 create/resume（`None` 新建、`Some` 恢复），`send(handle, prompt)` 等待子代理完成并取回最终文本。`SubagentHandle` 持有 `session_id` 与 `message_start_index`；`SubagentOutput` 持有 `text` 与 `message_end_index`。索引采用「可见条目」计数，仅计 UserMessage / ModelMessage / ModelThought / FunctionCall 四类事件。`RunnerSubagentHost` 为生产实现，持 `AgentRunnerBuilder` + 父 profile + 父取消令牌（父 cancel 级联取消子代理）。详见 [docs/spawn-agent.md](../../docs/spawn-agent.md)。
_避免使用_：子会话、从属会话

**工具（Tool / ToolContext）**：
工具 trait 与调用上下文。`Tool` 提供 `name()` / `description()` / `parameters_schema()` / `execute(ctx, args) -> anyhow::Result<Vec<ContentBlock>>`。`ToolContext` 携带 FunctionCall 的 `event_id` / `call_id` / `call_name`、事件发送通道与取消令牌，工具执行期间可 `send(Event)` 发射增量事件（如 `FunctionOutputDelta`）。
_避免使用_：工具接口、工具上下文

**工具循环（tool loop）**：
多轮工具调用。每轮：从 session 事件拼装 Message → 请求 LLM → 消费流 → 执行 tool call → 持久化 `FunctionResult` → 重复，直至无 tool_call 的 `TurnStop`。孤儿 `FunctionCall`（无配对结果）由 cancel 路径补发已取消结果，保证历史可被 LLM API 接受。
_避免使用_：工具调用循环、agent loop

**事件转换（convert）**：
Event ↔ rig Message 转换。`events_to_chat_history` 将 session Event 流转换为 rig Message 序列；`process_stream` 将 rig 流式 chunk 转换为 Event 流（`ModelMessageDelta` / `ModelThoughtDelta` 增量、`ModelThought` / `ModelMessage` 完整记录、`FunctionCall`、`TurnStop`）；`finish_reason_to_stop_reason` 将 finish_reason 映射为 `StopReason`。
_避免使用_：转换层、映射层
