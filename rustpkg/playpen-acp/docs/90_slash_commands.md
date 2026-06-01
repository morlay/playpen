# Slash Commands

[ACP Slash Commands 规范](https://agentclientprotocol.com/protocol/v1/slash-commands)

## AvailableCommands

Agent 通过 `AvailableCommandsUpdate` 通知向 Client 注册可用的斜杠命令。

### 内置命令

| 名称 | 说明 |
|------|------|
| `rewind` | 回退到上一轮用户消息，重新生成回复 |

### Skill 命令

以 `skill:{name}` 格式动态注册，来自 profile 中 `available_skills()`。

```
Client ← AvailableCommandsUpdate {
  commands: [
    { name: "rewind", description: "回退到上一轮用户消息，重新生成回复", input: Unstructured },
    { name: "skill:{name}", description, input: Unstructured }
  ]
}
```

发送时机（在 handler respond 之后）：
- `session/new` 完成后
- `session/load` 完成后（replay 之后）
- `session/resume` 完成后

## Skill 注入

详见 [Prompt Turn](./21_prompt_turn.md) 中的 Skill 注入章节。
