# playpen agent 构建计划

## 概述

在 playpen 基础上构建 gui-base 的 coding agent。技术栈不变（Rust / edition 2024）。

分两个子包：

| 包 | 路径 | 说明 |
|----|------|------|
| `playpen-agent-core` | `rustpkg/playpen-agent-core/` | LLM 调度核心，基于 rig |
| `playpen-agent-ui` | `rustpkg/playpen-agent-ui/` | GUI 封装，基于 gpui |

## 基础依赖

- **grep**：`rg`（ripgrep）
- **find**：`fd`（fd-find）
- **沙盒**：macOS `sandbox-exec`（通过 playpen CLI）
- **LLM**：`rig` + `rig-core`
- **HTML→Markdown**：`html-to-markdown-rs`

## 关键设计决策

### Provider 策略

仅支持 `"openai-completions"` API 类型（兼容 DeepSeek / OpenAI / 等）。

### 重试机制

API 调用包裹重试：指数退避、仅对网络/服务端错误重试。配置通用，同 pi。

### 配置目录结构

```
~/.config/playpen/
├── conf.d/
│   ├── sandbox.toml     # [filesystem] [network] [shell]
│   └── providers.toml   # [providers.*] provider + 模型定义
├── settings.toml        # default_provider, default_model, default_agent, retry 等
└── agent/
    └── {name}.md        # 系统提示词（YAML frontmatter + markdown 正文）
```

### Agent 系统提示词路径约定

| 路径 | 说明 |
|------|------|
| `~/.agents/AGENTS.md` | 全局 AGENTS.md |
| `~/.agents/skills/` | 全局 skill 目录 |
| `{PROJECT_ROOT}/AGENTS.md` | 项目级 AGENTS.md |
| `{PROJECT_ROOT}/.agents/skills/` | 项目级 skill 目录 |

### Session 管理

- Session 与 `project_root` 关联
- 每次 session 记录使用的 system prompt 和 tools JSON Schema（入库）
- **新提示词 → 必须创建新 session**，不可修改旧 session，避免上下文污染
- 支持归档（`archived_at` 时间戳），归档后可安全删除
- **上下文统计**：每条消息记录 `tokens` 数，从 API 响应的 `usage` 提取。提供 `context_usage()` 检测是否接近窗口上限

### API 请求/响应覆盖

| 维度 | 覆盖内容 |
|------|---------|
| 消息角色 | `System` / `User` / `Assistant` / `Tool` |
| 消息内容 | `Vec<ContentPart>`（一期只 text，预留 ImageUrl） |
| 工具调用 | `ToolCall.id` + `type: "function"` + `function(name, arguments, strict?)` |
| 推理思考 | `reasoning_content` 字段 |
| 用量统计 | `Usage { prompt_tokens, completion_tokens, total_tokens, reasoning_tokens, cached_prompt_tokens }` |
| 停止原因 | `FinishReason { Stop, Length, ToolCalls, ContentFilter }` — `Length` 用于检测上下文超限 |
| 请求参数 | `model`, `messages`, `tools`, `tool_choice`, `max_tokens`, `temperature`, `top_p`, `stop`, `presence_penalty`, `frequency_penalty`, `reasoning_effort`, `stream`, `user` |
| 流式 | `StreamChunk` / `StreamDelta` / `StreamToolCall` 全类型定义，`stream_completion()` 函数，**始终走流式**（日志也需要） |
| 工具配对 | `assert_tool_pairing()` 校验：Tool 消息必须紧跟匹配的 Assistant 消息 |
| 成本计价 | `ModelCost.input / output × usage` × `currency`（默认 CNY） |
| Strict 模式 | `ToolFunctionSchema.strict`，DeepSeek Beta 接口需设 `true` + `additionalProperties: false`

### 成本追踪

`ModelConfig.cost` 记录成本，`currency` 标明货币单位（默认 `"CNY"`）。

### 沙盒机制

- 文件系统工具（read / grep / edit / write / move / find）：**复用 `sandbox::config::validate_filesystem_path`** 做路径规则校验，遵从 `~/.config/playpen/conf.d/sandbox.toml` 的 `[filesystem]` 规则
- `bash` 工具：**委托 `playpen` 命令**执行，由 playpen 的 sandbox-exec + shell 规则兜底
- `webfetch` 工具：**不做拦截**（只读、强制 HTML→Markdown 格式转换，无安全隐患）

## 子计划

- [playpen-agent-core 详细计划](rustpkg/playpen-agent-core/.tasks/PLAN.md)
- [playpen-agent-ui 详细计划](rustpkg/playpen-agent-ui/.tasks/PLAN.md)
