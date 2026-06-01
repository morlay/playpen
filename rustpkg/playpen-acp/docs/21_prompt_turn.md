# Prompt Turn

[ACP Prompt Turn 规范](https://agentclientprotocol.com/protocol/v1/prompt-turn)

```
Client → session/prompt { session_id, prompt }
  │
  ├─ 1. 解析输入：提取 text content blocks
  │      → 检测 /skill:{name} 前缀 → 加载 SKILL.md 注入到输入前
  │
  ├─ 2. 启动 Agent Runner：runner.run(prompt_blocks)
  │      → SimpleRunner 内部多轮 tool use
  │
  ├─ 3. 消费 EventStream → SessionUpdate（map_event）
  │      → 见下方 Event → SessionUpdate 映射
  │
  └─ 5. 返回 PromptResponse { StopReason, usage }
```

## Event → SessionUpdate 映射

`EventMapper::new(working_dir).with_term_enabled(..).with_replay(..).with_default_message_id(..).map_event(&event)` 根据 `replay` 参数选择映射逻辑。

### Prompt 模式（`replay: false`）— 实时流式数据

| playpen Event | ACP SessionUpdate |
|---|---|
| `UserMessage` | —（已在 `handle_prompt` 中通过 `UserMessageChunk` 回显） |
| `ModelMessageDelta` | → `AgentMessageChunk`（增量文本） |
| `ModelMessage` | —（终态完整消息跳过，已通过 delta 流式传输） |
| `ModelThoughtDelta` | → `AgentThoughtChunk`（增量思考） |
| `ModelThought` | —（终态完整思考跳过） |
| `FunctionCall` | → `ToolCall(Pending)` + `ToolCallUpdate(InProgress)` |
| `FunctionOutputDelta` | → `ToolCallUpdate`（bash terminal_output） |
| `FunctionResult` | → `ToolCallUpdate(Completed / Failed, rawOutput)` |
| `TurnStop` | — |
| `TokenUsage` | → `UsageUpdate` |

### Replay 模式（`replay: true`）— 仅终态结果

| playpen Event | ACP SessionUpdate |
|---|---|
| `UserMessage` | → `UserMessageChunk` |
| `ModelMessageDelta` | —（回放无 delta） |
| `ModelMessage` | → `AgentMessageChunk`（完整消息） |
| `ModelThoughtDelta` | — |
| `ModelThought` | → `AgentThoughtChunk`（完整思考） |
| `FunctionCall` | → `ToolCall(Pending)` + `ToolCallUpdate(InProgress)` |
| `FunctionOutputDelta` | — |
| `FunctionResult` | → `ToolCallUpdate(Completed / Failed, rawOutput)` |
| `TurnStop` | — |
| `TokenUsage` | → `UsageUpdate` |

## Terminal Output 消息格式

当 Client 在 `initialize` 的 `_meta` 中声明 `terminal_output: true` 时，bash 工具的消息格式从标准 tool call 切换为 terminal 风格。

### 检测流程

```
initialize { _meta: { terminal_output: true } }
  → AcpState.terminal_output_enabled = true

FunctionCall(name="bash") 到达 map_function
  → terminal_output_enabled && name == "bash"?
     → 是：Terminal 格式
     → 否：标准 ToolCall 格式
```

### Terminal 格式（`terminal_output: true`）

```
[FunctionCall]  ToolCall(in_progress), content=[Terminal(id)], meta.terminal_info { terminal_id, cwd }
[Delta]         ToolCallUpdate, meta.terminal_output { terminal_id, data }
[Result]        ToolCallUpdate(completed), meta.terminal_exit { terminal_id, exit_code }
```

- `ToolCall.content` 包含 `Terminal` 对象，Client 可据此创建终端 UI
- `ToolCall.meta.terminal_info` 携带 `terminal_id` 和 `cwd`
- 工具执行的实时输出通过 `meta.terminal_output` 推送
- 工具完成后通过 `meta.terminal_exit` 通知退出码

### 标准格式（`terminal_output: false` 或未声明）

```
[FunctionCall]  ToolCall(pending → in_progress) + meta.tool_name
[Delta]         ToolCallUpdate(in_progress), content: [{ type: "text", text }]
[Result]        ToolCallUpdate(completed/failed), content: [Diff], rawOutput
```

- 输出以 text content 逐段推送
- 文件编辑结果附加 ACP Diff

## ToolCall

非 bash 工具调用分三阶段：
- 阶段 1：`ToolCall(status: pending, _meta: { tool_name })` — 声明工具调用
- 阶段 2：`ToolCallUpdate(status: in_progress)` — 背靠背标记执行中
- 阶段 3：`ToolCallUpdate(status: completed / failed, rawOutput)`

## Diff 内容

`edit` / `write` 工具完成时，`ToolCallUpdate` 附加 `content` 字段，包含 ACP Diff。

## TokenUsage 映射

`TokenUsage` → `UsageUpdate(used: total_token_count, size: total_token_count)`：

ACP `UsageUpdate` 的 `used` 表示当前上下文中已使用的 token 数，`size` 表示上下文窗口总大小。
当前实现中两者均使用 `total_token_count`。

## 工具执行错误判定

当 tool result 包含 `"Error"` 或 `"沙箱执行失败"` 时，状态标记为 `Failed`。

## StopReason 映射

| playpen StopReason | ACP StopReason |
|---|---|
| `EndTurn` | `end_turn` |
| `MaxTokens` | `max_tokens` |
| `MaxTurnRequests` | `max_turn_requests` |
| `Refusal` | `refusal` |
| `Cancelled` | `cancelled` |
| `Error(_)` | —（不产生 SessionUpdate） |

## session/cancel

```
Client → session/cancel { session_id }
  → 从 running_runners 缓存获取正在运行的 runner
  → runner.cancel()
  → 当前 prompt 流产生 TurnStop(Cancelled)
```

## Skill 注入

以 `/skill:{name}` 前缀触发：

```
/skill:foo              → 加载 foo 的 SKILL.md 全文
/skill:bar 帮我做 X     → 加载 bar 的 SKILL.md + 附带用户输入
```

注入格式：
```xml
<skill name="{name}">
{SKILL.md 全文}
</skill>

用户输入: {args}
```

- 未匹配（无前缀或 skill 不存在）→ 原样透传
