# 上下文映射

## 上下文

- [沙盒（Sandbox）](./rustpkg/playpen-sandbox/CONTEXT.md) — macOS seatbelt 隔离执行环境，提供文件系统、网络、Shell 的安全性约束
- [配置聚合](./rustpkg/playpen-config/CONTEXT.md) — TOML 分层合并、路径约定、模型与费用类型
- [Profile 管理](./rustpkg/playpen-profile/CONTEXT.md) — Agent 配置管理：profile 加载、skill 发现
- [工具抽象层](./rustpkg/playpen-toolkit/CONTEXT.md) — 文件系统、Shell 执行、网页抓取的能力抽象
- [Agent 核心](./rustpkg/playpen-agent/CONTEXT.md) — AgentRunner / AgentRunnerBuilder trait，基于 rig-core + playpen-session
- [ACP Agent](./rustpkg/playpen-acp/CONTEXT.md) — ACP 协议的 Agent 端实现，通过 stdio transport 与编辑器集成

## 关系

- **Sandbox → Config**：playpen-config 的 `AppConfig` 持有 `playpen_sandbox::config::Config`
- **Sandbox → Toolkit**：Toolkit 的 sandbox feature 通过 `Arc<dyn Sandbox>` 做路径/命令/域名权限检查
- **Config → Profile**：`AgentProfileLoader` 使用 `Dirs` 定位 profile 目录
- **Config → Agent**：`AgentRunner` 持有 `Settings`，`AgentRunnerBuilder` 接收 `Settings`
- **Content → Agent**：Agent 使用 `playpen-content` 的 `Event` / `ContentBlock` / `StopReason` 类型
- **Session → Agent**：Agent 通过 `SessionService` trait 持久化事件，`playpen-session` 提供 SQLite 实现
- **Profile → Agent**：AgentRunner 持有 `AgentProfile`，提供 instruction、model、skills；AgentRunnerBuilder 通过 `AgentProfileLoader` 查找 profile
- **Toolkit → Agent**：Agent 的 `tool/` 模块通过 `to_tool_definitions()` 将 Toolkit 工具转换为 rig-core `ToolDefinition`
- **ACP → Agent**：ACP 层通过 `AgentRunnerBuilder` 创建/恢复 runner，调用 `runner.run()` 驱动事件流
- **Sandbox ↔ ACP**：无直接依赖。通过 Agent 层间接使用
