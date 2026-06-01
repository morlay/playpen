# playpen-profile

Profile 管理。

## 职责

- `AgentProfile` trait：name / description / working_dir / model_profile / instruction / available_skills / tool_enabled / with_model_profile
- `AgentProfileLoader` trait：`agent_profiles(dirs)` 返回可用 profile 列表
- `LocalAgentProfileLoader` 从 `~/.config/playpen/profiles/{name}/profile.toml` + `instructions.md` 加载
