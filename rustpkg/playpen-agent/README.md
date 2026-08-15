# playpen-agent

运行时层——Agent 编排核心。自研 Agent Runner，基于 rig-core + playpen-session，负责把 profile、session、LLM 与工具串联成一次 Agent 执行。

## 职责

- `AgentRunner` / `AgentRunnerBuilder` trait：一次 session 的执行视图与工厂（`create` / `resume`）
- `SimpleRunner` / `SimpleRunnerBuilder`：基于 `playpen-session` 的生产实现
- `LlmClient` / `LlmConfig` / `ModelEnum`：rig-core 之上的 LLM 客户端，区分 DeepSeek / OpenAI 兼容协议
- `SubagentHost` / `RunnerSubagentHost`：子代理能力（`spawn` / `send`），支持嵌套
- `Tool` / `ToolContext`：工具 trait 与调用上下文
- tool loop：多轮工具调用直至 `TurnStop`
- `convert`：Event ↔ rig Message 转换与流式转换

## 核心模块

| 模块      | 职责                                                                                    |
| --------- | --------------------------------------------------------------------------------------- |
| `client`  | `LlmClient` / `LlmConfig` / `ModelEnum`：LLM 客户端与 DeepSeek / OpenAI 兼容协议区分      |
| `convert` | `events_to_chat_history` / `process_stream`：Event ↔ rig Message 转换与流式 chunk 转换    |
| `runner`  | `AgentRunner` / `AgentRunnerBuilder` / `SimpleRunner` / `SimpleRunnerBuilder`：执行视图、工厂与生产实现 |
| `subagent`| `SubagentHost` / `SubagentHandle` / `SubagentOutput` / `RunnerSubagentHost`：子代理能力    |
| `tool`    | `Tool` / `ToolContext` 与各工具实现（read / edit / write / grep / find / move / webfetch / bash，另含 spawn_agent） |
| `testing` | `FakeTool` / `TestProfile` / `make_runner`：可复用测试工具                               |
