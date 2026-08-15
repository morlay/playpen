# Playpen CLI

playpen 命令行入口，负责 clap 命令解析、分发到各子命令，并接线 session 数据库、日志与输出打印。

## 术语

**CLI 命令（Commands）**：
clap `Subcommand` 枚举，定义顶层命令：`run <script>`（执行脚本）、`ls-access <PATH>...`、`domain-access <DOMAIN>...`、`config`、`agent`（含 `session` 子命令）、`acp`，以及 `CatchAll`（`external_subcommand`，捕获未匹配命令）。
_避免使用_：子命令集、命令树

**Agent 子命令（AgentAction / SessionAction）**：
`agent` 的嵌套子命令。`AgentAction::Session` 承载 session 管理；`SessionAction::Get { id }` 查询单个 session，`SessionAction::List { limit, offset }` 列出 session。`agent` 本身参数：`--model`、`--profile`、`--thinking-level`、`-i`/`--interactive`、位置参数 `prompt`。
_避免使用_：agent 动作、会话动作

**命令接线（commands 模块）**：
子命令接线层。`access`（`ls-access` / `domain-access` 访问裁决查询）、`acp`（ACP 服务启动）、`agent`（交互式与单次 Agent 运行）、`config`（打印配置）、`run`（脚本执行与交互式沙盒 shell）、`session`（session `get` / `list`）。
_避免使用_：命令处理器、子命令实现

**session 数据库（db.rs）**：
session 数据库路径。`sessions_db_path()` 返回 `XDG_CACHE_HOME/playpen/sessions.db`；未设 `XDG_CACHE_HOME` 时用 `~/.cache`，`HOME` 亦未设时回退 `/tmp`。
_避免使用_：数据库、存储

**日志初始化（log.rs）**：
tracing 初始化。`init_logging()` 按 `PLAYPEN_LOG_DIR` 决定输出：设置时以 JSONL 写 `playpen-{时间戳}.jsonl`（默认 info），否则写 stderr（默认 warn）；过滤级别优先读环境变量（`EnvFilter`）。
_避免使用_：日志系统、日志模块

**输出打印（printer.rs）**：
CLI 输出打印。`Printer` 按 `Event` 类型以 `[role] text` 格式输出：`ModelMessageDelta` / `ModelThoughtDelta` 等增量缓冲后提交，`FunctionCall` / `FunctionResult` 输出工具名与参数。
_避免使用_：渲染器、输出器
