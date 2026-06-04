# 上下文映射

## 上下文

- [沙盒（Sandbox）](./rustpkg/sandbox/CONTEXT.md) — macOS seatbelt 隔离执行环境，提供文件系统、网络、Shell 的安全性约束
- [Agent Core](./rustpkg/playpen-agent-core/CONTEXT.md) — 沙盒优先的 coding agent 核心引擎，管理 Profile、Session、工具调用和 LLM 调度
- [ACP Agent](./rustpkg/playpen-acp/CONTEXT.md) — ACP 协议的 Agent 端实现，通过 stdio transport 与编辑器集成

## 关系

- **Sandbox → Agent Core**：Agent Core 依赖 Sandbox 的配置和校验。所有工具（Tool）的执行均通过沙盒规则进行路径、命令、域名的权限检查。
- **Agent Core → ACP Agent**：ACP Agent 消费 Agent Core 的 `AgentRunner` 和 `SessionStore` trait，通过事件映射将内部事件转为 ACP 协议消息。
- **Sandbox ↔ ACP Agent**：无直接依赖。ACP Agent 的 `build_state()` 中间接加载沙盒配置并注入 Agent Core。
