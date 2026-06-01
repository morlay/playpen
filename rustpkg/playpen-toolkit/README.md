# playpen-toolkit

工具抽象层。

## 职责

- `FileSystem` / `Terminal` / `Fetcher` trait：工具能力抽象（均使用 `anyhow::Result`）
- `Toolkit` 聚合：`Toolkit::defaults(cwd)` + `Toolkit::with_sandbox(sandbox)`
- `sandbox` feature：`SandboxFileSystem` / `SandboxTerminal`，通过 `Arc<dyn Sandbox>` 校验访问权限
- `native/`：默认实现（`NativeFileSystem` / `NativeTerminal` / `NativeFetcher`）
