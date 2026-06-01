# Profile 管理

Agent 配置管理：profile 加载、skill 发现。

## 术语

**AgentProfile**：
session 维度的配置实体。包含名称、描述、工作目录、model 配置、system prompt（instructions）、可用技能列表、工具开关。通过 `instructions()` 将 profile 配置的 instruction、项目 AGENTS.md、环境 XML、可用 skills 合并为完整 system prompt。

**AgentProfileLoader**：
`AgentProfile` 的加载器。扫描 `~/.config/playpen/profiles/{name}/profile.toml`，读取 TOML 配置和同目录下的 `instructions.md`，构造 `AgentProfile` 实例。
_避免使用_：profile 仓库、配置工厂

**Skill**：
技能包 trait，由 SKILL.md 文件（YAML frontmatter + markdown body）定义。提供 `metadata()`（名称、描述等）、`location()`（文件路径）、`source()`（来源枚举）、`instructions()`（markdown 内容）。

**Source**：
技能来源枚举。`Global`（全局 `~/.agents/skills/`）、`Project`（项目 `.agents/skills/`）。项目级同名 skill 覆盖全局。
