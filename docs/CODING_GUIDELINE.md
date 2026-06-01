# Rust 开发手册

## 代码约定

### 测试

- 单元测试用独立文件 `{name}_test.rs`，通过 `#[path]` 声明，不与实现文件混在一起。
- 集成测试放 `tests/` 目录。
- 先保证工具可测（文件系统 mock、HTTP mock），再测全流程。

### 序列化

- JSON 统一 snake_case，不使用 camelCase。

### 类型组织

- 每个文件只放一类类型（`model.rs`、`event.rs` 等）。
- 避免泛化的 `types.rs` 命名。

### 新增 crate

1. 在 `Cargo.toml` 添加 workspace member
2. 创建 README.md 说明职责
3. 不破坏现有依赖方向（见 [ARCHITECTURE.md](./ARCHITECTURE.md)）

### 风格

- 不做过度抽象和防御性编程。先写能跑的最小版本，再逐步补齐。
- 外部依赖全部可 mock（LLM mock、in-mem session、fake toolkit）。

## 错误处理

- **严禁 silent error**：所有 `Result` / `Option` 不得用 `let _ =` 或 `.ok()` 静默丢弃。
- **内部传播，入口打印**：中间层使用 `anyhow::Result` 或自定义 enum 向上传播，不在中间层 `eprintln!`。仅在 CLI 入口（`main.rs`）或 ACP handler 顶层打印 / 返回错误。
- **通知失败须 warn**：`cx.send_notification` 等非关键路径失败时用 `tracing::warn!` 记录，不得静默。
- **不在库 crate 中使用 `eprintln!`**：库代码使用 `tracing`，由入口统一配置输出。

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
cargo test -p playpen-agent

# 单测试函数
cargo test test_agent_runner_stop -p playpen-agent
```

## 沙箱（sandbox）

- shell 解析使用 `flash`（POSIX shell 解析器），生成 AST 后 walk 节点判断规则，不自写解析器。
- 无 doc tests（`[lib] doctest = false`），因沙盒环境不允许 `/var/folders` 写权限。

## 新增依赖

- 优先使用 workspace 已有的依赖版本
- 仅测试用的 mock 库 → `[dev-dependencies]`
