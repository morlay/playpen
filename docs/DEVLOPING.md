# 开发指南

## 常用命令

```
# 首次构建前需下载 seatbelt 策略
cd rustpkg/sandbox && just download

just rust build playpen  # 构建 playpen（自动编译依赖）
just rust test           # 测试所有包
just rust lint           # clippy 所有包
just rust fmt            # 格式化所有包
just rust install playpen  # 安装 playpen
```

## 工具链顺序

1. `just rust lint`
2. `just rust test`
3. `just rust fmt`

## 配置

- 全局：默认 `~/.config/playpen.toml`，可通过 `PLAYPEN_GLOBAL_CONFIG_PATH` 环境变量覆盖
- 项目级：`{cwd}/.playpen.toml`
- 默认模板：`example/.sandbox.toml`

两个配置文件的 `access`/`allow` 字符串按顺序拼接（全局在前，项目级在后）。

### 规则语法

#### 文件系统 / 网络（`[filesystem]` / `[network]`）

| 前缀 | 含义     |
| ---- | -------- |
| `rw` | 读写允许 |
| `r-` | 只读     |
| `--` | 拒绝     |
| 无   | 允许     |

模式兼容 `.gitignore` 语义：
- `/path` → 绝对路径前缀
- `~/path/` → HOME 相对路径
- `dir/` → 目录匹配
- `*.ext` → 后缀通配
- `.name` → 精确文件名匹配

沙箱默认拒绝——未匹配的路径/域名均被拦截。无规则时无限制。

#### Shell（`[shell]`）

| 前缀 | 含义 |
| ---- | ---- |
| `!`  | 拒绝 |
| 无   | 允许 |

Shell 命令权限黑/白名单，与文件系统规则语法独立。

## playpen 用法

```bash
# 执行命令
playpen exec git status

# 执行 shell 脚本
playpen run 'git status && echo done'

# 验证文件路径权限
playpen validate path /tmp/test /etc/passwd

# 验证网络域名权限
playpen validate domain api.example.com

# 解释当前生效规则
playpen explain

# 生成 zed agent 配置（预览）
playpen setup zed-agent code

# 生成并写入（自动备份 .bak）
playpen setup zed-agent code --write
```

### zed agent 配置映射

读工具（grep/find_path/read_file）已禁用，读操作由 shell + seatbelt 兜底。仅映射写工具权限：

| sandbox 规则       | 写工具 (write_file) | 删工具 (delete_path) |
| ------------------ | ------------------- | -------------------- |
| 允许（`rw`/无前缀） | `always_allow`      | —                    |
| 拒绝（`--`）       | `always_deny`       | `always_deny`        |
| 只读（`r-`）       | `always_deny`       | `always_deny`        |
| 终端工具           | 固定委托给 `playpen`         |

> 写入 settings.json 时仅替换 `agent` 字段，保留文件中其他所有内容和注释。

## 项目结构

```
rustpkg/
├── sandbox/            # 沙盒策略库（解析、验证、seatbelt 生成）
├── playpen-zed-agent/  # zed agent 配置生成库
└── playpen/            # 命令行入口（依赖 sandbox + playpen-zed-agent）
```

## 测试

| 库                | 单元测试                 | 集成测试     |
| ----------------- | ------------------------ | ------------ |
| sandbox           | `tests/config_test.rs` 等 | `tests/sandbox_test.rs` |
| playpen-zed-agent | `src/zed_test.rs` 内联   | —            |
| playpen           | —                        | —            |
