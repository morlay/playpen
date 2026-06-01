# playpen

基于 ACP（Agent Client Protocol）的 coding agent 后端。可选 macOS seatbelt 沙箱隔离。

## 快速开始

```bash
# 安装
just rust build playpen

# ACP 模式（供 Zed 等编辑器连接）
playpen acp

# 或直接 CLI 使用
playpen agent "你好"
playpen agent --profile=code -i
```

## 功能

- **ACP 协议** — 与任何支持 ACP 的编辑器集成
- **沙箱隔离** — macOS seatbelt 沙箱，保障 LLM 工具执行安全（可选）
- **工具集** — 文件读写/编辑、Shell 执行、网页抓取、grep/find
- **全自定义 Profile** — 每个 profile 可独立配置 system instruction、模型、thinking level、可用工具，数量不限
- **项目编码规范** — 项目根目录的 `AGENTS.md` 自动注入 system prompt，约束 agent 行为
- **Skills** — 可复用技能包，在 prompt 中通过 `/{skill-name}` 直接引用
- **对话持久化** — SQLite 存储对话历史

## 配置

```toml
# ~/.config/playpen/settings.toml
[model_providers.deepseek]
api_key = "${DEEPSEEK_API_KEY}"

[sandbox]
enabled = true

[sandbox.filesystem]
access = ["rw .", "r- ~/"]
```

Profile 是完全自定义的——每个 profile 可以指定不同的 system instruction、模型、推理深度、启用的工具集。你可以为不同场景创建多个 profile：

```
~/.config/playpen/profiles/
├── code/
│   ├── profile.toml           # 编程助手
│   └── instructions.md        # system instruction（可选，覆盖 TOML 中 instruction 字段）
├── review/
│   └── profile.toml           # 代码审查
└── terminal/
    └── profile.toml           # 终端操作
```

一个典型的 `profile.toml`：

```toml
description = "编程助手"
instruction = "你是一个编程助手..."  # 当同目录下不存在 instructions.md 时生效

[default_model_profile]
model = "deepseek/deepseek-v4-flash"
thinking_level = "off"
temperature = 0.7
top_p = 0.9
```

也可将 instruction 写在同目录的 `instructions.md` 中（优先于 TOML 内联字段），便于长文本管理。

项目根目录下的 `AGENTS.md` 会自动注入 system prompt，无需在 profile 中配置。

详细文档见 [docs](./docs/ARCHITECTURE.md)。
