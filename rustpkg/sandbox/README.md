# sandbox

macOS Seatbelt（`sandbox-exec`）封装——将 playpen 配置规则编译为 seatbelt profile，并强制执行文件系统、网络与 Shell 访问控制。

## 安全模型

**默认拒绝**——未匹配的路径 / 域名 / 命令均被拦截。仅在规则中显式声明后才放行。

规则定义在 TOML 配置中，多来源按序合并（后加载覆盖先加载）：全局 `~/.config/playpen/settings.toml` + `~/.config/playpen/conf.d/*.toml`，项目级 `<cwd>/.playpen.toml`。

## 规则语法

### 文件系统规则

```toml
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

| 前缀 | 含义     |
| ---- | -------- |
| `rw` | 读写允许 |
| `r-` | 只读     |
| `--` | 拒绝     |
| 无   | 允许     |

模式兼容 `.gitignore` 语义：

| 模式        | 匹配方式               |
| ----------- | ---------------------- |
| `/etc/`     | 绝对路径前缀           |
| `~/dir/`    | HOME 相对路径前缀      |
| `.`         | cwd（项目目录）        |
| `./rel`     | cwd 相对路径前缀       |
| `dir/`      | 任意位置目录匹配       |
| `.env`      | 精确文件名匹配         |
| `*.pem`     | 通配后缀               |
| `prefix*`   | 通配前缀               |

### 网络规则

```toml
[network]
access = """
*.example.com
!api.example.com
"""
```

| 前缀 | 含义     |
| ---- | -------- |
| `!`  | 拒绝     |
| 无   | 允许     |

支持 `*` 通配域名。

### Shell 规则

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

| 前缀 | 含义 |
| ---- | ---- |
| `!`  | 拒绝 |
| 无   | 允许 |

按命令名 + 非 flag 参数前缀匹配，规则按参数数量降序排列（更具体的规则优先命中）。

## 核心模块

| 模块       | 职责                                         |
| ---------- | -------------------------------------------- |
| `config`   | 规则解析（`parse_filesystem_string` 等）      |
| `policy`   | 规则分类，生成 `PolicyClassification`        |
| `seatbelt` | 将 `PolicyClassification` 编译为 seatbelt profile |
| `shell`    | Shell 命令解析与权限校验                      |
| `sandbox`  | 对 `sandbox-exec` 的封装调用                  |
