# Rust 开发手册

## 项目结构

```
rustpkg/
├── sandbox/                  # 沙盒执行引擎
├── playpen-agent-core/       # Agent 核心引擎
├── playpen-acp/              # ACP 协议 Agent 端
├── playpen-zed-agent/        # zed 编辑器集成（生成 tool_permissions 和 AGENTS.md）
└── playpen/                  # CLI 入口
```

## 上下文边界

项目按领域划分为三个上下文，详见 [CONTEXT-MAP.md](../CONTEXT-MAP.md)：

| 上下文 | 职责 | 依赖 |
|--------|------|------|
| **Sandbox** | 沙盒配置 + Seatbelt 策略生成 | 无 |
| **Agent Core** | Profile / Session / 工具调度 / LLM 调用 | Sandbox |
| **ACP Agent** | ACP 协议适配 + Session 持久化 | Agent Core |

- 跨上下文通信只通过 trait object，不依赖具体类型。
- 新增领域概念时更新对应上下文的 `CONTEXT.md`。
- 新增子模块时评估是否需要新建上下文。

## 代码约定

### 测试

- 单元测试用独立文件 `{name}_test.rs`，通过 `#[path]` 声明，不与实现文件混在一起。
- 集成测试放 `tests/` 目录。
- 先保证工具可测（文件系统 mock、HTTP mock），再测全流程。

### 序列化

- JSON 统一 snake_case，不使用 camelCase。

### 类型组织

- 每个文件只放一类类型（`model.rs`、`message.rs`、`session.rs` 等）。
- 避免泛化的 `types.rs` 命名。

### 新增 crate

1. 在 `Cargo.toml` 添加 workspace member
2. 创建 README.md 说明职责
3. 属于哪个上下文 → 放在对应上层目录下

### 风格

- 不做过度抽象和防御性编程。先写能跑的最小版本，再逐步补齐。
- 复用现有依赖（sandbox、ignore、regex、grep、quick-xml），不重复造轮子。
- 外部依赖全部可 mock（HTTP mock、LLM mock）。

## 常用命令

```bash
just rust lint     # clippy 全 workspace（-D warnings）
just rust test     # 测试全 workspace
just rust fmt      # 格式化全 workspace
just rust build playpen  # 构建 CLI
```

## 测试命令

```bash
# 单 crate
cargo test -p playpen-agent-core

# 单测试文件
cargo test --test agent_runner_test

# 单测试函数
cargo test test_agent_runner_stop --test agent_runner_test
```

## 新增依赖

- 使用不熟悉的 crate 前，先读其文档和源码中的测试用例，理解正确用法再动手，忌凭直觉猜测语义。
- 能同时用于正常编译和测试的 → `[dependencies]`
- 仅测试用的 mock 库 → `[dev-dependencies]`
- 优先使用 workspace 已有的依赖版本
