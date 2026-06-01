# Session Setup

[ACP Session Setup 规范](https://agentclientprotocol.com/protocol/v1/session-setup)  
[ACP Session Config Options 规范](https://agentclientprotocol.com/protocol/v1/session-config-options)

涵盖 session 的创建、加载、恢复、关闭、列举、删除，以及配置选项。

## session/new — Creating a Session

```
Client → session/new { cwd }
  → 读取第一个 profile（agent_profiles().first()）
  → session_service.create() 创建 session（session_id 由 playpen-session 生成）
  → SessionMeta 存入 session state + AcpState 缓存
  ← { sessionId, configOptions }
  → AvailableCommandsUpdate（skills 清单）
```

### Working Directory（cwd）

- 必须是绝对路径
- 作为 session 的文件系统上下文根目录

### Session ID

`sessionId` 由 session_service.create() 生成（UUID v7），Client 后续通过此 ID 发送 prompt、加载、关闭等操作。

## session/load — Loading Sessions

```
Client → session/load { sessionId, cwd }
  → builder.resume(sid)：从 session state 恢复 profile metadata
  → 同步重放 conversation history（runner.replay()）
  ← { configOptions }
  → AvailableCommandsUpdate
```

session 不存在时返回错误。**重放在 response 之前同步完成**，确保客户端在收到 response 时已收到全部历史消息。

重放时 `map_event` 以 `replay: true` 模式映射，过滤规则见 [21_prompt_turn.md](./21_prompt_turn.md#replay-模式replay-true-仅终态结果)。

## session/resume — Resuming Sessions

```
Client → session/resume { sessionId, cwd }
  → builder.resume(sid)
  ← { configOptions }
  → AvailableCommandsUpdate
```

与 `session/load` 的区别：**不重放历史消息**。session 不存在时静默成功。

| | session/load | session/resume |
|---|---|---|
| 重放历史消息 | ✅ | — |
| 发送 ConfigOptionUpdate | ✅ | ✅ |
| 发送 AvailableCommandsUpdate | ✅ | ✅ |

## session/close — Closing Active Sessions

```
Client → session/close { sessionId }
  ← {}
```

## session/list — Listing Sessions

[ACP Session List 规范](https://agentclientprotocol.com/protocol/v1/session-list)

```
Client → session/list {}
  → 从 session service（playpen-session SQLite）查询全量 session
  ← { sessions: SessionInfo[] }
```

## session/delete — Deleting Sessions

[ACP Session Delete 规范](https://agentclientprotocol.com/protocol/v1/session-delete)

```
Client → session/delete { sessionId }
  → 从 session service 删除
  ← {}
```

## Session Config Options

[ACP Session Config Options 规范](https://agentclientprotocol.com/protocol/v1/session-config-options)

### set_config_option

```
Client → session/set_config_option { session_id, config_id, value }
  → "mode"          → 更新 profile_name，重置 thought_level → cache_meta
  → "model"         → 更新 model_key → cache_meta
  → "thought_level" → 更新推理深度 → cache_meta
  ← ConfigOptions（全量，currentValue 反映最新状态）
```

### ConfigOptions

| configId | category | 说明 |
|---|---|---|
| `mode` | `mode` | profile 列表，从 `profile_resolver.list_profiles()` 动态获取 |
| `model` | `model` | 可用模型列表（固定：`deepseek/deepseek-v4-flash` / `deepseek/deepseek-v4-pro`） |
| `thought_level` | `thought_level` | 推理深度（`off`/`high`/`max`） |

### Mode 切换

通过 `session/set_config_option` 切换 mode（profile）：

```
set_config_option("mode", new_profile_name)
  → 更新 SessionMeta.profile_name → cache_meta
  → 重置 thought_level 为 "off"
```

对客户端表现为「同 session_id 内热切换 mode」。system prompt 由 `AgentProfile.instructions()` 固化，session 创建后不可修改。

### set_config_option 处理

`session/new` / `load` / `resume` 响应中的 `configOptions` 返回后，Client 可能发送连续多个 `session/set_config_option` 同步配置，每个独立响应。

AvailableCommands 在 session setup 完成（handler respond 之后）发送，不依赖 debounce。
