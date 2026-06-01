# 工具抽象层

Agent 工具的能力抽象：文件系统、Shell 执行、网页抓取。支持原生实现和沙箱包装。

## 术语

**Toolkit**：
工具聚合体。持有 `FileSystem`、`Terminal`、`Fetcher` 三个能力 trait 的 `Arc` 引用。提供 `defaults(cwd)` 创建原生实现，`with_sandbox(sandbox)` 叠加沙箱包装层。工具 schema 由各 Tool 实现（`playpen-agent` 中）通过 `parameters_schema()` 独立声明。

**FileSystem**：
文件系统操作抽象 trait。提供 `read`、`edit`、`write`、`grep`、`find`、`move` 六个方法。原生实现 `NativeFileSystem` 直接操作本地文件系统；沙箱包装 `SandboxFileSystem` 在每次操作前调用 `playpen_sandbox::access()` 做路径权限裁决。
_避免使用_：文件管理器、目录服务

**Terminal**：
Shell 执行抽象 trait。提供 `exec(cmd)` 方法，返回 `UnboundedReceiver<CommandOutput>` 流式输出。原生实现 `NativeTerminal` 使用 `tokio::process::Command`；沙箱包装 `SandboxTerminal` 通过 `playpen_sandbox::wrap_command()` 校验命令合法性。

**CommandOutput**：
Shell 命令输出的流式变体。包含 `Stdout`（标准输出）、`Stderr`（标准错误）、`Exited { code }`（退出码）、`Cancelled`（取消）。
_避免使用_：进程输出、shell 结果

**Fetcher**：
网页抓取抽象 trait。提供 `fetch(opt)` 方法，支持超时、大小限制、MIME 类型筛选。原生实现 `NativeFetcher` 使用 `reqwest`。text/html 响应自动转为 Markdown。
_避免使用_：下载器、爬虫

**沙箱包装（Sandbox Wrapper）**：
通过 feature `sandbox` 条件编译的包装层。`SandboxFileSystem` 和 `SandboxTerminal` 分别包裹对应 trait，在每次操作前通过 `Arc<dyn Sandbox>` 做权限检查。不启用 sandbox feature 时，Toolkit 使用原生实现，无沙箱开销。
