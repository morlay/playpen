# ACP Agent

ACP（Agent Client Protocol）的 Agent 端实现，通过 stdio transport 与编辑器（如 Zed）集成。

## 术语

**ACP**：
Agent Client Protocol，定义编辑器（Client）与 Agent（Server）之间的通信协议。通过 JSON-RPC 风格的请求/通知交换 session 管理、prompt 交互、配置更新等消息。
_参见_：[agentclientprotocol.com](https://agentclientprotocol.com)

**Transport**：
ACP 的通信传输层。当前实现为 stdio——Agent 进程通过 stdin 接收请求、stdout 发送响应，进程即连接。

**事件映射（Event Mapping）**：
将 Agent Core 的内部 `AgentEvent`（如 `TextDelta`、`ToolCallStart`）转换为 ACP 协议定义的 `SessionUpdate` 消息的桥接逻辑。

**Session 持久化（Session Persistence）**：
在 `SessionManager`（内存）之上增加文件存储层。每个 Session 序列化为 `{store_dir}/{id}.json`，进程启动时自动恢复，每次变更后写回。

**Slash Command**：
以 `/` 前缀触发的快捷指令，映射到已扫描的技能（Skill）。由 ACP Agent 在 session 初始化时上报给编辑器。
