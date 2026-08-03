# spawn_agent：子代理工具设计与 ACP 集成

> 实现状态：已落地。playpen-agent 提供 `SubagentHost` 接缝与 `SpawnAgentTool`；playpen-acp 在
> `handle_prompt` 注入宿主，并把子代理会话信息通过 `ToolCallUpdate._meta` 传给 ACP client（Zed）。

## 目标

让主 agent 通过 `spawn_agent` 工具生成子代理（独立的持久化 session）完成定义明确的子任务，
取回最终输出；子代理会话可被 ACP client（Zed）加载查看、跳转、续聊。

## 接口（playpen-agent）

```rust
// rustpkg/playpen-agent/src/subagent.rs

pub const SUBAGENT_SESSION_INFO_META_KEY: &str = "subagent_session_info";

/// 子代理会话句柄（runner 私有持有，供 send 使用）。
pub struct SubagentHandle {
    pub session_id: String,
    /// send 前的可见条目数（本轮 turn 起点索引）。
    pub message_start_index: usize,
}

/// 一次子代理运行的输出。
#[derive(Debug)]
pub struct SubagentOutput {
    pub text: String,
    /// 完成后可见条目总数 - 1（终点索引，inclusive）。
    pub message_end_index: usize,
}

#[async_trait]
pub trait SubagentHost: Send + Sync {
    /// session_id=None 新建（继承父 profile）；Some 恢复既有会话续聊。
    async fn spawn(&self, label: &str, session_id: Option<&str>) -> anyhow::Result<SubagentHandle>;
    /// 发送 prompt 并等待完成。运行失败走 Err，但 handle.session_id 依然有效。
    async fn send(&self, handle: &SubagentHandle, prompt: &str) -> anyhow::Result<SubagentOutput>;
}
```

生产实现 `RunnerSubagentHost { builder: Arc<dyn AgentRunnerBuilder>, profile: Arc<dyn AgentProfile> }`：

- `spawn(None)`：`builder.create(profile)` 新 session；`spawn(Some(sid))`：`builder.resume(sid)`。
  创建/恢复后立即 `child.with_subagent_host(self.builder.clone())` 注入宿主——**嵌套传播用同一
  builder 重建 host（绑定子 runner 自己的 profile），链条不断**。
- `send`：`runner.run([message])` → 收集最后一条 `ModelMessage` 文本；`TurnStop` 的
  `Error/Cancelled/Refusal` 转 `Err`。**多轮工具循环**：`TurnStop` 不提前结束（带 tool_call 的
  中间轮 TurnStop 之后还有 FunctionResult 与下一轮），只有流结束才是真结束；子代理**只调工具
  未输出文本**时，回退到最后一次工具输出（如 bash 结果），主 agent 仍能看到实际产出。
- **取消级联**：`RunnerSubagentHost` 持有父 runner 的 `CancellationToken`，`send` 期间用
  `tokio::select!` 监听父 cancel——父会话取消时立即 `handle.runner.cancel()` 级联取消子代理
  （子代理的 LLM 请求随之终止，不再空转）。`with_subagent_host_typed` 的取消令牌与父 runner
  **共享**（而非新建），保证 ACP `cancel` 通知落到注册的注入后 runner 时，同一令牌被取消。

## 工具（playpen-agent/tool/spawn_agent.rs）

```json
输入 { "label": "短标签", "message": "任务描述", "session_id": "可选，续聊" }
输出 { "session_id": "...", "output": "最终文本" }   // 模型可见（annotations 不展示给模型）
```

- 工具**铁律**：成败都返回 `Ok` + annotations（`Err` 通道会丢失 annotations 与 session_id）：
  ```json
  annotations: {
    "_meta.subagent_session_info": { "session_id": ..., "message_start_index": ..., "message_end_index": ... },
    "exit_code": 0 | 1
  }
  ```
- 运行中发一条 `FunctionOutputDelta` 进度提示（只发射不持久化）。

## 注入链

1. `AgentRunner` trait 增加 `with_subagent_host(&self, builder) -> Box<dyn AgentRunner>`（与
   `with_profile` 同构）；`SimpleRunner` 持有 `subagent_host: Option<Arc<dyn SubagentHost>>`，
   `build_run_tools()` 统一构建运行工具列表——Toolkit 默认工具（read/edit/write/grep/find/
   move/webfetch/bash）+ 有宿主时附加 `SpawnAgentTool`。主 runner 与子代理 runner（均为
   `SimpleRunner`）走同一方法，**子代理工具注册与主 agent 完全一致**；最终按
   `profile.tool_enabled` 过滤发生在 `build_tools_and_defs`（默认 profile 放行全部工具）。
2. `SimpleRunner` 提供具体类型版本 `with_subagent_host_typed` / `with_profile_typed`，供包装
   类型（测试 FakeRunner）复用。
3. playpen-acp：`AcpState.builder` 由 `Box` 升为 `Arc`；`Context::with_subagent_host(runner)`
   在 **`handle_prompt`（resume 后）注入**——这是唯一真正运行 `run()` 的入口；子代理 resume
   走同一路径，天然支持嵌套。

## 索引语义（关键）

`message_start_index / message_end_index` 对齐 Zed 的 `AgentThreadEntry` 计数：**仅计四类
产生 UI 条目的事件**——`UserMessage / ModelMessage / ModelThought / FunctionCall`。
`StateUpdate / Model*Delta / FunctionOutputDelta / FunctionResult / TurnStop` 不计数
（`subagent::entry_count`）。

- `start` = spawn 后、send 前的可见条目数
- `end` = send 完成后可见条目数 - 1

## ACP 集成（playpen-acp）

事件流（对应 Zed client 行为）：

| 阶段 | playpen 消息 | Zed 行为 |
|---|---|---|
| FunctionCall | `ToolCall(spawn_agent, title=label, Pending, meta={tool_name})` + `ToolCallUpdate(InProgress)` | 主 thread 新增卡片 |
| 运行中 | `ToolCallUpdate(content=[进度文本])`（FunctionOutputDelta） | 卡片流式内容 |
| 完成 | `ToolCallUpdate(Completed/Failed, content=[输出], meta={subagent_session_info})` | 解析 meta → `is_subagent()` → 内联预览 → `load_session` 加载子代理会话 |
| 失败 | 同上但 `exit_code=1` → Failed | 子代理卡片失败态，仍可点入查看 |

实现要点：

- `event_mapper::map_function_result` 把 annotations 中 `_meta.*` 前缀（复用 `acp_content.rs`
  的 `extract_acp_annotations`）还原为 `ToolCallUpdate.meta`——live 与 replay 双模式均生效，
  保证主会话重载后跳转入口不丢失。
- `display::build_tool_title` 对 `spawn_agent` 用 `label` 作为标题。
- Zed 查看子代理完整会话走既有 `session/load` 链路（`handle_load_session` + replay），无需新协议。

## 测试

- `subagent_test.rs`：`entry_count` 只计四类事件；spawn/send 成功与输出收集；resume 路径索引；
  LLM 错误传播。
- `spawn_agent_test.rs`：工具成功/失败 annotations 契约（session_id 保留、exit_code）、
  resume 路径、spawn 失败走 Err。
- `event_mapper_test.rs`：live + replay 双模式下 `subagent_session_info` meta 提取（锁 G1）。
- `agent_test.rs` FakeRunner / `acp_state_test.rs` StubRunner 补齐 `with_subagent_host`。

## 日志（排查子代理「无输出」）

子代理路径全程带 tracing 日志（`PLAYPEN_LOG_DIR` 设置后以 JSONL 输出，默认 info 级别）：

| 阶段 | 日志 | 级别 |
|---|---|---|
| 创建/恢复 | `subagent spawn 开始/完成`（含 label、mode、profile、model、start 索引） | info |
| 发消息 | `subagent send 开始`（含 prompt 长度） | info |
| 模型输出 | `subagent 收到模型文本/思考`（含长度） | debug |
| 工具调用 | `subagent 调用工具`（含工具名与参数）、`subagent 工具结果` | info/debug |
| 结束 | `subagent turn 结束`（含 stop_reason）、`subagent send 完成/失败` | info/warn |
| 工具侧 | `spawn_agent 工具调用/子代理就绪/子代理完成` | info |

**「啥都没」的常见根因**：子代理只执行工具调用、未输出文本消息 → `send` 返回提示文本
「（子代理已完成任务，未返回文本消息）」并打 warn 日志，主 agent 不会看到空结果。
排查时先看 `subagent spawn 完成`（确认子代理 session 建立）→ `subagent 调用工具`（确认模型
在干活）→ `subagent send 完成`（确认结果回传）。

## 已知边界（首版不做）

- 无嵌套深度限制（Zed 有 MAX_SUBAGENT_DEPTH；playpen 无 thread 实体可跨 resume 保持深度）。
- 运行中不实时转发子代理事件流（与 Zed 原生行为一致：最终文本一次性返回）。

## 取消语义

父会话 `cancel`（ACP `cancel` 通知 → `handle_cancel_notification` → 注册的 runner `.cancel()`）
会**级联取消所有进行中的子代理**：`RunnerSubagentHost.send` 在 `tokio::select!` 中监听父
`CancellationToken`，触发即 `handle.runner.cancel()`。子代理被取消后其 run 循环补发已取消的
`FunctionResult`（`emit_cancelled_results`），session 无孤儿 tool_call，可安全 resume。
