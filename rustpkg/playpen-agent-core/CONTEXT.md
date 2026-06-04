# Agent Core

沙盒优先的 coding agent 核心引擎，管理 Profile、Session、工具调用和 LLM 调度。

## 术语

**Agent**：
沙盒优先的 coding agent，通过 ACP 协议对外集成，无独立 GUI。负责接收用户输入、调度 LLM、调用工具、管理会话生命周期。所有工具调用均受沙盒约束。
_避免使用_：bot、助手、LLM 客户端

**工具（Tool）**：
Agent 可供 LLM 调用的 function calling 操作：`read`、`grep`、`edit`、`write`、`move`、`find`、`bash`、`webfetch`。按类别分为文件操作、Shell 执行、网络请求三类，每类的沙盒校验方式对应沙盒配置的三个维度。
_避免使用_：插件、命令、能力

**Profile**：
Agent 的身份档案，包含 Profile 提示词、启用的工具列表、技能开关等。一个 Profile 下可创建多个 Session。
_避免使用_：配置、预设

**Profile 提示词**：
Profile 中撰写的 markdown 文本，定义 Agent 的角色身份和行为准则。是 Session 系统提示词的组成部分。

**Session**：
基于某个 Profile 创建的一次对话实例。Session 系统提示词在创建时固化不可修改；模型和思考等级可运行时切换。当系统提示词变更时须创建新 Session，避免上下文缓存污染。
_避免使用_：对话、线程、聊天

**Session 系统提示词**：
由 Profile 提示词、AGENTS.md、已加载技能、环境信息拼装而成的完整 system prompt。在 Session 创建时固化，不可后续修改。
_避免使用_：上下文、角色定义

**AGENTS.md**：
遵循 agents.md 社区标准的代理指令文件。分全局级（`~/.agents/`）和项目级（项目根目录），在 Session 创建时合并到系统提示词中。

**技能（Skill）**：
遵循 agentskills.io 规范的模块化能力包，按目录组织于 `~/.agents/skills/` 或 `<project>/.agents/skills/`。在 Session 创建时按需注入系统提示词。

**Provider**：
LLM API 提供商，包含 API 地址、密钥、兼容类型等配置。一个 Provider 下可注册多个 Model。

**Model**：
具体的 LLM 模型，关联于某个 Provider。包含上下文窗口大小、成本单价、推理能力等属性。

**Registry**：
Provider 和 Model 的注册中心，负责模型发现、过滤和 API 客户端构建。

**Workspace**：
Agent 的工作区上下文，封装项目根目录、沙盒配置和文件系统规则。提供路径解析、文件读写和沙盒校验能力，所有文件的读/写操作均通过 Workspace 进行安全检查。
_避免使用_：项目目录、工作目录
