# playpen-agent-core

playpen agent 核心引擎实现。

## 依赖

- `playpen-agent-protocol` — 接口协议
- `sandbox` — 沙盒执行
- `rig-core` — LLM 调度

## 模块

- `types` — re-export protocol 类型 + core 内部类型
- `agent` — CoreAgentRunner（AgentRunner 实现）+ agent_loop
- `session` — SessionManager（SessionStore 实现）
- `providers` — ProviderRegistry（ModelStore 实现）+ DeepSeek 内置配置
- `agent_profile` — AgentProfileManager（AgentProfileStore 实现）+ 提示词构建
- `tools` — read / grep / edit / write / move / find / bash / webfetch
- `config` — 配置加载（conf.d 合并）
- `sandbox` — sandbox crate 封装

## 测试

87 个测试覆盖：单元测试 + 工具集成测试 + agent loop 测试 + agent runner 测试
