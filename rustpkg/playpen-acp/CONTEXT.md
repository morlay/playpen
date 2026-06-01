# ACP Agent

ACP（Agent Client Protocol）的 Agent 端实现，通过 stdio transport 与编辑器（如 Zed）集成。纯协议适配层。

## 术语

**ACP**：
Agent Client Protocol，定义编辑器（Client）与 Agent（Server）之间的通信协议。通过请求/通知交换 session 管理、prompt 交互、配置更新等消息。

**AcpState**：
ACP 服务端运行时状态。持有 `AgentRunnerBuilder` + runner 缓存 + 待应用配置 + terminal 输出标记。
runner 通过 `register_running` / `unregister_running` 管理生命周期。
**不持有 `ConnectionTo<Client>`**——`cx` 由 dispatch 层通过参数链传递给各 handler，避免全局共享连接导致的串发问题。

**PendingConfig**：
client 通过 `session/set_config_option` 设置的覆盖参数暂存结构。包含 `profile_name`、`model_key`、`thought_level` 三个可选字段，在首次 prompt 时应用到 runner。
_避免使用_：待处理配置、延迟配置

**事件映射（Event Mapping）**：
纯函数 `Event → Vec<SessionUpdate>` 转换。通过 `EventMapper` 结构体（builder 模式）配置上下文。实时 prompt 时流式推送 delta 事件，回放时仅推送终态事件。

**Skill 注入**：
`/skill:{name}` 指令触发。从 `AgentProfile::available_skills()` 查找 skill，读取 `SkillInfo::location` 文件内容注入 prompt。
由 `process_slash_commands()` 统一处理 text block 中的 slash commands，返回拆分后的 `Vec<ContentBlock>`。

**Rewind**：
`/rewind` slash command。触发 `AgentRunner::rewind()`，调用 `session_service.rewind(event_id)` 回退到指定事件（最近一条 user message）之前。
由 `process_slash_commands()` 检测并标记，`run_prompt_loop` 中调用 `runner.rewind()`。

**Terminal Output 模式**：
Client 声明 `terminal_output = true` 时，bash 工具以 terminal 风格推送——`terminal_info`（含 `terminal_id` / `cwd`）、`terminal_output`（含 `data`）、`terminal_exit`（含 `exit_code`）三种 meta。
_避免使用_：终端模式、TTY 模式

**Block 转换**：
- `to_agent_blocks()` — AcpContentBlock → AgentContentBlock（含 Blob 解码）
- `to_acp_blocks()` — AgentContentBlock → AcpContentBlock（含 Blob 编码）
两个转换位于 `acp_content.rs`，与 `text_from_blocks()` 共用。

## 架构要点

- `cx`（`ConnectionTo<Client>`）**不存储在 AcpState 中**，由 ACP 框架传入 `handle_dispatch`，通过参数链传给各 handler。
- `send_notification()` 和 `send_available_commands()` 是自由函数，接受 `&ConnectionTo<Client>` 作为首个参数。
- slash commands（`/rewind`、`/skill:xxx`）由 `process_slash_commands()` 批量处理，返回处理后 blocks + rewind 标记，不内联在 handler 中。
