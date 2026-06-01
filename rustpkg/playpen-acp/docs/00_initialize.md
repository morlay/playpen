# Initialize

[ACP Initialization 规范](https://agentclientprotocol.com/protocol/v1/initialization)

```
Client → initialize { protocolVersion, clientCapabilities, clientInfo, _meta }
  → 协商协议版本
  → 读取 _meta.terminal_output → 存入 AcpState
  → 构建 AgentCapabilities
  ← InitializeResponse { protocolVersion, agentCapabilities, agentInfo, authMethods }
```

## 协议版本协商

- Client 在 `protocolVersion` 中声明支持的最新主版本（当前为 `1`）
- Agent 若支持该版本则原样返回；否则返回自身支持的最新版本
- Client 若无法匹配 Agent 返回的版本，应断开连接并提示用户
- playpen-acp 固定返回 `protocol_version: 1`

## ClientCapabilities → 状态写入

| 字段 | 来源 | 说明 |
|---|---|---|
| `fs.readTextFile` | `ClientCapabilities.fs.read_text_file` | 仅能力声明，不影响流程 |
| `fs.writeTextFile` | `ClientCapabilities.fs.write_text_file` | 仅能力声明 |
| `terminal` | `ClientCapabilities.terminal` | 仅能力声明 |
| `_meta.terminal_output` | `ClientCapabilities._meta` | 非标准扩展；设为 `true` 时，bash 工具调用以 terminal 风格推送 |

`_meta.terminal_output` 存入 `AcpState.terminal_output_enabled`，后续 prompt 流程据此决定 bash 工具结果的通知格式。终端输出的完整消息格式见 [Terminal Output 消息格式](./21_prompt_turn.md#terminal-output-消息格式)。

## AgentCapabilities 声明

| 能力 | 值 | 对应协议方法 |
|---|---|---|
| `loadSession` | `true` | `session/load` |
| `promptCapabilities.image` | `false` | 不支持 Image content block |
| `promptCapabilities.audio` | `false` | 不支持 Audio content block |
| `promptCapabilities.embeddedContext` | `false` | 不支持 Resource content block |
| `sessionCapabilities.list` | `{}` | `session/list` |
| `sessionCapabilities.resume` | `{}` | `session/resume` |
| `sessionCapabilities.close` | `{}` | `session/close` |
| `sessionCapabilities.delete` | `{}` | `session/delete` |
| `authMethods` | `[]` | 无需认证 |

基线方法（`session/new`、`session/prompt`、`session/cancel`、`session/update`）无需在 capabilities 中声明——所有 Agent 必须支持。

## Implementation Info

```json
{
  "agentInfo": {
    "name": "playpen-acp",
    "title": "Playpen ACP Agent",
    "version": "0.1.0"
  }
}
```

- `name` 用于程序化/逻辑引用
- `title` 面向终端用户显示
- `version` 来自 `CARGO_PKG_VERSION`，用于调试和遥测
