# 沙盒（Sandbox）

基于 macOS sandbox-exec（Seatbelt）的隔离执行环境，为 Agent 的工具调用提供文件系统、网络、Shell 的安全性约束。

## 术语

**沙盒（Sandbox）**：
基于 macOS sandbox-exec 的隔离执行环境。通过沙盒配置声明访问约束，Agent 的所有工具调用须先通过沙盒校验。
_避免使用_：容器、虚拟环境、chroot

**沙盒配置（Sandbox Config）**：
声明访问约束的规则集，包含三个维度：文件系统访问、网络访问、可用命令。配置可分布在全局（`~/.config/playpen/settings.toml`、`~/.config/playpen/conf.d/*.toml`）和项目级（`.playpen.toml`），运行时合并。
_避免使用_：策略、权限

**文件系统规则（Filesystem Rule）**：
声明文件路径的访问权限。前缀 `rw` 为读写、`r-` 为只读、`--` 为拒绝（优先级最高）。路径模式兼容 `.gitignore` 语义（绝对路径、HOME 相对路径、通配符等）。未匹配任何规则时默认拒绝。
_避免使用_：ACL、权限位

**网络规则（Network Rule）**：
声明域名的访问权限。前缀 `!` 为拒绝，无前缀为允许，支持 `*` 通配。未匹配任何规则时默认拒绝。

**Shell 规则（Shell Rule）**：
声明可执行的命令。按命令名 + 非 flag 参数前缀匹配，支持 `!` 拒绝特定子命令。同时控制是否允许管道（`|`）和多命令串联（`&&`/`;`）。

**Seatbelt Profile**：
macOS sandbox-exec 读取的 SBPL 格式策略文件。由文件系统规则编译生成，实现 last-match 覆盖语义和 `require-not` 拒绝嵌入。是 macOS 平台的适配层，其他平台用不同实现。
_避免使用_：安全策略、沙盒策略
