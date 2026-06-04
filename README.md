# playpen

基于 macOS `sandbox-exec` 的命令行沙盒工具，为 zed agent 提供文件系统与命令执行的安全隔离。

## 安装

```bash
just rust install rustpkg/playpen
```

## 使用

### 命令执行

```bash
playpen git status
playpen run 'git status && echo done'
```

所有命令均通过 sandbox-exec 执行，按 shell 规则进行命令权限检查。

### 查询规则

```bash
# 查询文件系统规则
playpen ls-access ./*

# 查询域名规则
playpen domain-access github.com api.github.com
```

### zed agent 集成

```bash
# 预览生成的 tool_permissions 和 AGENTS.md
playpen setup zed-agent code

# 写入（自动备份 .bak）
playpen setup zed-agent code --write
```

写入目标：
- `~/.config/zed/settings.json` — tool_permissions
- `~/.config/zed/AGENTS.md` — agent 提示词（需配置 `[agents.zed]`）

写入时仅替换 `agent` 字段，保留 JSONC 注释及其他配置。

## 配置

参见 [example/sandbox.toml](./example/sandbox.toml)。配置语法详见 [sandbox README](./rustpkg/sandbox/README.md#规则语法)。

### 文件系统 / 网络

```toml
[network]
access = """
*.example.com
!api.example.com
"""

[filesystem]
access = """
rw .
r- ~/
rw .cache/
r- /etc/
-- .env
-- *.pem
"""
```

### Shell

```toml
[shell]
allow_pipe = true
allow_multiple = false
allow = """
git *
!git push *
just *
"""
```

### Agent 提示词

```toml
[agents.zed]
system = """
## 语言
- 所有思考、分析、推理过程使用中文。
"""
guidelines = [
    "内置工具能满足的操作，严禁使用 terminal()",
]
```

`playpen setup zed-agent code --write` 时生成 `~/.config/zed/AGENTS.md`，包含 system prompt 和工具使用指南。

## 开发

参见 [docs/DEVLOPING.md](./docs/DEVLOPING.md)。
