# playpen

playpen CLI 入口——命令解析、分发与各子命令接线，交互层最外层。

## 职责

- `main.rs`：clap 命令解析与分发（`Commands` / `AgentAction` / `SessionAction`）
- `commands/`：`access` / `acp` / `agent` / `config` / `run` / `session` 各子命令接线
- `db.rs`：session 数据库路径；`log.rs`：tracing 初始化；`printer.rs`：输出打印

## 命令

| 命令                                                            | 说明                                          |
| --------------------------------------------------------------- | --------------------------------------------- |
| `playpen run <script>`                                          | 经沙盒校验后执行脚本（`sh -c`）               |
| `playpen ls-access <PATH>...`                                   | 查询路径访问裁决（`rw` / `r-` / `--`）        |
| `playpen domain-access <DOMAIN>...`                             | 查询域名访问裁决（`ALLOW` / `DENY`）          |
| `playpen config`                                                | 打印合并后的配置（TOML）                       |
| `playpen agent [-i] [--model M] [--profile P] [--thinking-level L] [prompt]` | 运行 Agent；`-i` 交互式，否则单次执行 |
| `playpen agent session get <id>`                                | 查询 session 事件与统计（JSON）                |
| `playpen agent session list [--limit N] [--offset N]`           | 列出 session（JSON）                           |
| `playpen acp`                                                   | 启动 ACP（stdio transport）                    |
| `playpen`                                                       | 无子命令时进入交互式沙盒 shell                  |
| `playpen <cmd>...`（catch-all）                                 | 按外部命令执行（经沙盒校验，`sh -c`）           |
