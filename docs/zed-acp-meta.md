# Zed ACP 私有 Meta 扩展

> 分析基于 `acp_thread` + `agent_servers`，ACP 协议版本 `agent-client-protocol-schema` v0.14.0。

## 背景

ACP (Agent Communication Protocol) 的标准消息类型（`ToolCall`、`ToolCallUpdate`、`AvailableCommand` 等）均预留了一个通用扩展字段：

```
meta: Option<acp::Meta>  // 本质是 HashMap<String, serde_json::Value>
```

Zed 利用这个通道传递标准协议未覆盖的私有元数据。定义分布在 `acp_thread/src/acp_thread.rs`（运行期消息）和 `agent_servers/src/acp.rs`（能力协商 + 终端输出扩展）两个 crate 中。

> 参考 ADR: [`docs/adr/0002-acp-terminal-output-extension.md`](adr/0002-acp-terminal-output-extension.md)

## 总览

| # | Key | 挂载消息 | 方向 | 值类型 | 用途分类 |
|---|---|---|---|---|---|
| 1 | `tool_name` | `ToolCall.meta` | Agent → Client | `string` | 协议缺陷补偿 |
| 2 | `command_category` | `AvailableCommand.meta` | Agent → Client | `"native"` \| `"mcp"` | UI 增强 |
| 3 | `subagent_session_info` | `ToolCall.meta` | Agent → Client | `object` | 子 agent 编排 |
| 4 | `sandbox_authorization` | `ToolCall.meta` | Agent → Client | `object` | 安全/权限 |
| 5 | `sandbox_fallback_authorization` | `ToolCall.meta` | Agent → Client | `object` | 安全/权限 |
| 6 | `sandbox_not_applied` | `ToolCall.meta` | Agent → Client | `object` | 安全/权限 |
| 7 | `refusal_fallback_model` | `RetryStatus.meta` | Client → Agent (记录) | `string` | 错误恢复 |
| 8 | `terminal_output` | `ClientCapabilities._meta` | Client → Server | `true` | 终端输出扩展协商 |
| 9 | `terminal-auth` | `ClientCapabilities._meta` / `AuthMethod.meta` | 双向 | `true` / `object` | 终端认证扩展 |
| 10 | `terminal_info` | `ToolCall.meta` | Server → Client | `object` | 终端输出扩展 |
| 11 | `terminal_exit` | `ToolCallUpdate.meta` | Server → Client | `object` | 终端输出扩展 |

---

## 1. `tool_name` — 工具的程序化名称

### 用途

ACP 标准 `ToolCall` 只有一个 `title`（人类可读标签），缺少机器可读的 name 字段（类似 OpenAI 的 `function.name` 或 Anthropic 的 `tool.name`）。Zed 通过 `tool_name` 来传递这个名称，用于：

- 判断是否为 `spawn_agent` 工具调用
- 在 UI 中按名称归类/过滤工具调用

### 代码位置

- 定义: `acp_thread.rs:65`
- 提取: `acp_thread.rs:68` `tool_name_from_meta()`
- 构造: `acp_thread.rs:76` `meta_with_tool_name()`

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01abc123",
    "title": "Generate README for project",
    "kind": "edit",
    "meta": {
      "tool_name": "write_file"
    }
  }
}
```

### 消费端

在 `ToolCall::from_acp()` (line 536) 中提取并存入 `ToolCall.tool_name` 字段。

---

## 2. `command_category` — 斜杠命令来源分组

### 用途

标记 slash command 的来源，使 Zed 的自动补全弹出框能按类别分组展示。只有 Zed 原生 agent 会标注此元数据；外部 ACP agent 的命令不带此字段，统一归入"其他"分组。

### 代码位置

- 定义: `acp_thread.rs:82`
- 类型: `acp_thread.rs:88` `CommandCategory` 枚举
- 提取: `acp_thread.rs:116` `command_category_from_meta()`
- 构造: `acp_thread.rs:112` `meta_with_command_category()`

### 报文示例

```json
{
  "type": "available_commands_update",
  "available_commands_update": {
    "commands": [
      {
        "name": "compact",
        "display_name": "/compact",
        "meta": {
          "command_category": "native"
        }
      },
      {
        "name": "fetch",
        "display_name": "/fetch"
      }
    ]
  }
}
```

### 枚举值

| 值 | 含义 |
|---|---|
| `"native"` | Zed 内置命令（如 `/compact`） |
| `"mcp"` | 来自 MCP server 的 prompt 命令 |

### 解码规则

未知或缺失 → `None`（统一归入"其他"分组，不报错）。

---

## 3. `subagent_session_info` — 子 agent 会话追踪

### 用途

当父 agent 通过 `spawn_agent` 工具生成一个子 agent 时，父 agent 在 meta 中嵌入子 session 的上下文。使得 UI 能够将子 agent 的输出正确关联到父 agent 的对话流中。

### 代码位置

- 定义: `acp_thread.rs:123`
- 类型: `acp_thread.rs:267` `SubagentSessionInfo`
- 提取: `acp_thread.rs:278` `subagent_session_info_from_meta()`

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01spawn",
    "title": "Research topic with sub-agent",
    "kind": "fetch",
    "meta": {
      "subagent_session_info": {
        "session_id": "sub-session-001",
        "message_start_index": 0
      }
    }
  }
}
```

### SubagentSessionInfo 结构

| 字段 | 类型 | 说明 |
|---|---|---|
| `session_id` | `string` (acp::SessionId) | 子 agent 的 session ID |
| `message_start_index` | `number` | 子 agent 此次 turn 的起始消息索引 |
| `message_end_index` | `number` (可选) | 子 agent 返回后的结束消息索引 |

---

## 4. `sandbox_authorization` — 沙箱提权审批

### 用途

当 agent 请求超出当前沙箱权限的操作时（如网络访问、文件写入、脱离沙箱），通过此 meta 传递审批详情给 Zed 的 UI，显示审批对话框供用户选择 AllowOnce / AllowThread / AllowAlways / Deny。

### 代码位置

- 定义: `acp_thread.rs:126`
- 类型: `acp_thread.rs:163` `SandboxAuthorizationDetails`
- 提取: `acp_thread.rs:200` `sandbox_authorization_details_from_meta()`
- 构造: `acp_thread.rs:193` `meta_with_sandbox_authorization()`

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01sandbox",
    "title": "Install dependencies",
    "kind": "execute",
    "meta": {
      "sandbox_authorization": {
        "command": "npm install",
        "network_hosts": ["registry.npmjs.org"],
        "network_all_hosts": false,
        "allow_git_access": false,
        "allow_fs_write_all": false,
        "unsandboxed": false,
        "write_paths": ["/home/user/project/node_modules"],
        "reason": "需要下载 npm 包到 node_modules 目录"
      }
    }
  }
}
```

### SandboxAuthorizationDetails 结构

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `command` | `string` (可选) | 无 | 请求提权的命令 |
| `network_hosts` | `string[]` | `[]` | 需访问的特定主机（如 `github.com`、`*.npmjs.org`） |
| `network_all_hosts` | `bool` | `false` | 是否请求任意网络访问（别名 `network` 兼容旧版） |
| `allow_git_access` | `bool` | `false` | 是否请求访问 `.git` 目录 |
| `allow_fs_write_all` | `bool` | `false` | 是否请求任意文件写入 |
| `unsandboxed` | `bool` | `false` | 是否请求完全脱离沙箱 |
| `write_paths` | `string[]` | `[]` | 允许写入的特定路径 |
| `reason` | `string` | `""` | agent 提供的审批理由，显示在对话框中 |

### 关联常量

```rust
pub enum SandboxPermission {
    AllowOnce,     // id = "allow"
    AllowThread,   // id = "allow_thread"
    AllowAlways,   // id = "allow_always"
    Deny,          // id = "deny"
}
```

注意 `AllowThread` 和 `AllowAlways` 在 ACP 协议层面都使用 `PermissionOptionKind::AllowAlways`，区别仅在于 option id 字符串。

---

## 5. `sandbox_fallback_authorization` — 沙箱创建失败的降级审批

### 用途

当 OS 沙箱无法创建时（如 Linux 的 Bubblewrap 未安装），向用户展示降级审批对话框。与 `sandbox_authorization` 的区别在于：前者是 agent 主动请求提权，后者是沙箱基础设施本身不可用。

### 代码位置

- 定义: `acp_thread.rs:208`
- 类型: `acp_thread.rs:220` `SandboxFallbackAuthorizationDetails`
- 提取: `acp_thread.rs:239` `sandbox_fallback_authorization_details_from_meta()`
- 构造: `acp_thread.rs:230` `meta_with_sandbox_fallback_authorization()`

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01fallback",
    "title": "Run build script",
    "kind": "execute",
    "meta": {
      "sandbox_fallback_authorization": {
        "command": "make build",
        "reason": "bwrap: 未在 PATH 中找到。请安装 Bubblewrap 以启用沙箱。"
      }
    }
  }
}
```

### SandboxFallbackAuthorizationDetails 结构

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `command` | `string` (可选) | 无 | 请求执行的命令 |
| `reason` | `string` | `""` | 沙箱创建失败的人类可读原因 |

### 关联常量

`SANDBOX_FALLBACK_RETRY_OPTION_ID = "retry"` — 降级审批对话框中的"重试"选项 ID。

---

## 6. `sandbox_not_applied` — 沙箱未应用的原因

### 用途

当线程启用了沙箱功能，但某个特定命令没有在沙箱中执行时，记录原因。在 UI 中以警告形式展示，同时用于向用户和 agent 解释情况。

### 代码位置

- 定义: `acp_thread.rs:251`
- 提取: `acp_thread.rs:260` `sandbox_not_applied_from_meta()`
- 构造: `acp_thread.rs:253` `meta_with_sandbox_not_applied()`
- 值类型: `terminal.rs:192` `SandboxNotAppliedReason`

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01unsandboxed",
    "title": "Run untrusted script",
    "kind": "execute",
    "meta": {
      "sandbox_not_applied": {
        "DisabledForThisThread": null
      }
    }
  }
}
```

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_02bwrapfail",
    "title": "Compile",
    "kind": "execute",
    "meta": {
      "sandbox_not_applied": {
        "ErrorLinuxWsl": "BwrapNotFound"
      }
    }
  }
}
```

### SandboxNotAppliedReason 枚举

| 变体 | 值 (JSON) | 说明 |
|---|---|---|
| `DisabledForThisThread` | `"DisabledForThisThread"` | 用户为此线程禁用了沙箱 |
| `ErrorLinuxWsl(BwrapNotFound)` | `{"ErrorLinuxWsl": "BwrapNotFound"}` | `bwrap` 二进制未找到 |
| `ErrorLinuxWsl(SetuidRejected)` | `{"ErrorLinuxWsl": "SetuidRejected"}` | `bwrap` 是 setuid-root，Zed 拒绝执行 |
| `ErrorLinuxWsl(SandboxProbeFailed)` | `{"ErrorLinuxWsl": "SandboxProbeFailed"}` | `bwrap` 存在但无法创建沙箱（通常是无特权用户命名空间被禁用） |
| `ErrorLinuxWsl(Other(msg))` | `{"ErrorLinuxWsl": {"Other": "..."}}` | 其他错误，带人类可读描述 |

此枚举是跨平台一致的——所有变体在所有平台上都可序列化/反序列化，确保序列化后的元数据不依赖于写入时的操作系统。

---

## 7. `refusal_fallback_model` — 模型拒绝后的回退模型名

### 用途

当 agent 返回 `StopReason::Refusal`（拒绝回答问题）时，Zed 可选择回退到另一个模型重新尝试。此 meta 记录回退时使用的模型名称，供 telemetry 或后续分析使用。

区别于前 6 个 key，此 key 记录的**不是 agent 发送给 client 的信息**，而是 client 自身行为（选择回退模型）的记录。

### 代码位置

- 定义: `acp_thread.rs:1679`
- 提取: `acp_thread.rs:1685` `refusal_fallback_model_from_meta()`
- 构造: `acp_thread.rs:1681` `meta_with_refusal_fallback()`
- 挂载: `RetryStatus.meta` (line 1676)

### 报文示例

```json
{
  "retry_status": {
    "attempt": 2,
    "max_attempts": 3,
    "meta": {
      "refusal_fallback_model": "gpt-4-turbo"
    }
  }
}
```

### 值类型

纯字符串，值为回退模型的标识符。

---

## 8. `terminal_output` — 客户端能力协商（终端输出扩展）

### 用途

ACP 官方协议未定义 bash 的终端风格输出格式。Zed client 通过 `ClientCapabilities._meta` 中的 `terminal_output` 标记向 server 声明支持该扩展。PlayPen server 在 `initialize` 阶段读取该标记，后续事件映射中条件启用终端输出格式。

### 代码位置

- `agent_servers/src/acp.rs:741` — client capabilities 构造
- `agent_servers/src/acp.rs:2480` — 测试断言

### 报文示例

```json
{
  "capabilities": {
    "terminal": true,
    "_meta": {
      "terminal_output": true,
      "terminal-auth": true
    }
  }
}
```

### 值类型

`true`（布尔值）。存在且为 `true` 表示客户端支持非标准终端输出格式。

### 触发效果

当 `terminal_output` 启用且工具名为 `bash` 时，server 使用以下三种非标准 meta 替代标准 `ToolCallContent::Terminal` 格式：

| 消息 | 标准格式 | 扩展格式 |
|---|---|---|
| ToolCall | `ToolCallContent::Terminal(...)` | `ToolCall.meta.terminal_info` |
| ToolCallUpdate | — | `ToolCallUpdate.meta.terminal_output` |
| ToolCallUpdate | — | `ToolCallUpdate.meta.terminal_exit` |

非终端模式下，Zed 展示依赖两处约定：`build_tool_title()` 拼装可读标题，`wrap_code_block()` 将输出包裹为代码块。

---

## 9. `terminal-auth` — 终端认证扩展

### 用途

Client 通过 `ClientCapabilities._meta` 中的 `terminal-auth` 标记声明支持终端认证。Server 可以通过 `AuthMethod.meta` 中的 `terminal-auth` 携带认证命令的元数据，Client 据此构建终端认证任务。

### 代码位置

- 声明: `agent_servers/src/acp.rs:742` — client capabilities 构造
- 消费: `agent_servers/src/acp.rs:1028` — Gemini agent 的 terminal-auth 构造
- 解析: `agent_servers/src/acp.rs:1461` `meta_terminal_auth_task()`
- 类型: `agent_servers/src/acp.rs:1467` `MetaTerminalAuth` 内部结构

### 报文示例

**Client Capabilities 声明：**

```json
{
  "capabilities": {
    "auth": {
      "terminal": true
    },
    "_meta": {
      "terminal-auth": true
    }
  }
}
```

**Server AuthMethod 响应（Gemini agent 示例）：**

```json
{
  "auth_methods": [
    {
      "type": "agent",
      "id": "gemini-oauth",
      "label": "Login",
      "description": "Login with your Google or Vertex AI account",
      "meta": {
        "terminal-auth": {
          "label": "gemini /auth",
          "command": "/path/to/gemini",
          "args": ["--experimental-acp"],
          "env": {}
        }
      }
    }
  ]
}
```

### MetaTerminalAuth 结构

| 字段 | 类型 | 说明 |
|---|---|---|
| `label` | `string` | 认证任务的显示标签 |
| `command` | `string` | 认证命令的可执行文件路径 |
| `args` | `string[]` | 命令参数（可选） |
| `env` | `object` | 环境变量（可选） |

Client 根据此结构调用 `acp_thread::build_terminal_auth_task()` 构造 `SpawnInTerminal` 任务并在新终端中执行认证流程。

---

## 10. `terminal_info` — 终端注册信息

### 用途

当 `terminal_output` 扩展启用且工具为 `bash` 时，server 在 `ToolCall` 的 meta 中附带 `terminal_info`，告知 client 创建一个显示用终端。client 在 `agent_servers` 层预拦截处理此 meta，在 `acp_thread` 正式处理 `ToolCall` 之前先注册终端。

### 代码位置

- 消费: `agent_servers/src/acp.rs:3996-4032` — `handle_session_update()` 预处理

### 报文示例

```json
{
  "type": "tool_call",
  "tool_call": {
    "tool_call_id": "toolu_01bash",
    "title": "echo 'hello'",
    "kind": "execute",
    "meta": {
      "terminal_info": {
        "terminal_id": "term-001",
        "cwd": "/home/user/project"
      }
    }
  }
}
```

### terminal_info 结构

| 字段 | 类型 | 说明 |
|---|---|---|
| `terminal_id` | `string` | 终端唯一 ID |
| `cwd` | `string` (可选) | 终端工作目录 |

### 处理流程

1. `handle_session_update()` 收到 `SessionUpdate::ToolCall`
2. 检查 `tc.meta` 中是否包含 `terminal_info`
3. 若包含，调用 `TerminalBuilder::new_display_only()` 创建显示用终端
4. 通过 `thread.on_terminal_provider_event(TerminalProviderEvent::Created)` 注册到 `acp_thread`
5. 然后将更新正常转发给 `acp_thread.handle_session_update()`

之后 `ToolCallContent::Terminal` 标准处理路径就能通过 `terminal_id` 找到已注册的终端。

---

## 11. `terminal_exit` — 终端退出状态

### 用途

当 `terminal_output` 扩展启用时，server 在 `ToolCallUpdate` 的 meta 中附带 `terminal_exit`，通知 client 终端已退出并携带退出码和信号信息。

### 代码位置

- 消费: `agent_servers/src/acp.rs:4067-4089` — `handle_session_update()` 后处理

### 报文示例

```json
{
  "type": "tool_call_update",
  "tool_call_update": {
    "tool_call_id": "toolu_01bash",
    "meta": {
      "terminal_exit": {
        "terminal_id": "term-001",
        "exit_code": 0
      }
    }
  }
}
```

```json
{
  "type": "tool_call_update",
  "tool_call_update": {
    "tool_call_id": "toolu_02signal",
    "meta": {
      "terminal_exit": {
        "terminal_id": "term-002",
        "exit_code": null,
        "signal": "SIGKILL"
      }
    }
  }
}
```

### terminal_exit 结构

| 字段 | 类型 | 说明 |
|---|---|---|
| `terminal_id` | `string` | 终端唯一 ID |
| `exit_code` | `number` (可选) | 进程退出码 |
| `signal` | `string` (可选) | 终止信号名（如 `SIGKILL`） |

Client 据此构建 `acp::TerminalExitStatus` 并通过 `TerminalProviderEvent::Exit` 通知 `acp_thread`。

### 注意

`terminal_output`（输出流）与 `terminal_exit`（退出状态）是**独立且可叠加**的——同一条 `ToolCallUpdate` 可以同时包含 `terminal_output` 和 `terminal_exit` meta。输出流先处理，再处理退出状态。

### 用途

当 agent 返回 `StopReason::Refusal`（拒绝回答问题）时，Zed 可选择回退到另一个模型重新尝试。此 meta 记录回退时使用的模型名称，供 telemetry 或后续分析使用。

区别于前 6 个 key，此 key 记录的**不是 agent 发送给 client 的信息**，而是 client 自身行为（选择回退模型）的记录。

### 代码位置

- 定义: `acp_thread.rs:1679`
- 提取: `acp_thread.rs:1685` `refusal_fallback_model_from_meta()`
- 构造: `acp_thread.rs:1681` `meta_with_refusal_fallback()`
- 挂载: `RetryStatus.meta` (line 1676)

### 报文示例

```json
{
  "retry_status": {
    "attempt": 2,
    "max_attempts": 3,
    "meta": {
      "refusal_fallback_model": "gpt-4-turbo"
    }
  }
}
```

### 值类型

纯字符串，值为回退模型的标识符。

---

## 扩展点总览

### 使用的消息类型

| ACP 消息类型 | 挂载的私有 meta |
|---|---|
| `ClientCapabilities._meta` | `terminal_output`, `terminal-auth` |
| `AuthMethod.meta` | `terminal-auth` |
| `ToolCall` (SessionUpdate) | `tool_name`, `subagent_session_info`, `sandbox_authorization`, `sandbox_fallback_authorization`, `sandbox_not_applied`, `terminal_info` |
| `ToolCallUpdate` (SessionUpdate) | `terminal_output` (流式数据), `terminal_exit` |
| `AvailableCommand` | `command_category` |
| `RetryStatus` (Zed 内部) | `refusal_fallback_model` |

### 设计模式

所有私有 meta 的存取都遵循相同的模式：

```rust
// 1. 定义 key 常量
pub const FOO_META_KEY: &str = "foo";

// 2. 构造辅助函数
pub fn meta_with_foo(value: FooValue) -> acp::Meta {
    acp::Meta::from_iter([(FOO_META_KEY.into(), serde_json::to_value(value).unwrap_or_default())])
}

// 3. 提取辅助函数
pub fn foo_from_meta(meta: &Option<acp::Meta>) -> Option<FooValue> {
    meta.as_ref()
        .and_then(|m| m.get(FOO_META_KEY))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}
```

简单值（`tool_name`、`command_category`、`refusal_fallback_model`）使用 `v.as_str()` 直接提取；
复杂结构（`subagent_session_info`、`sandbox_authorization`、`sandbox_fallback_authorization`、`sandbox_not_applied`）使用 `serde_json::from_value` 反序列化。
