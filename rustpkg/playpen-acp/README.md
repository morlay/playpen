# playpen-acp

ACP（Agent Client Protocol）协议适配层。将编辑器通过 ACP 协议接入 `playpen-agent`。

## Crate 边界

- 只做协议适配，不含 agent 逻辑
- Session 持久化由 `playpen-session` 处理
- 事件类型在 `playpen-agent` 中定义

## 术语

见 [CONTEXT.md](./CONTEXT.md)。

## Handler 文档

| 协议方法 | 文档 | 实现 |
|---|---|---|
| `initialize` | [docs/00_initialize.md](./docs/00_initialize.md) | `handler/initialize.rs` |
| `session/new` / `session/load` / `session/resume` / `session/close` / `session/list` / `session/delete` / `set_config_option` | [docs/10_session_setup.md](./docs/10_session_setup.md) | `handler/session_setup.rs` |
| `session/prompt` / `session/cancel` | [docs/21_prompt_turn.md](./docs/21_prompt_turn.md) | `handler/prompt_turn.rs` |
| 斜杠命令 / AvailableCommands | [docs/90_slash_commands.md](./docs/90_slash_commands.md) | `state.rs` |
