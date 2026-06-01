# seatbelt profile 生成机制

## 背景

playpen 使用 macOS 的 [sandbox-exec](https://www.unix.com/man-page/osx/1/sandbox-exec/) 实现文件系统沙箱。sandbox-exec 读取 SBPL（Sandbox Profile Language）格式的策略文件，决定进程的文件、网络、IPC 等操作是否被允许。

seatbelt 规则采用 **last-match** 机制：对多个匹配同一操作的规则，**最后一个匹配的规则生效**。这意味着输出顺序至关重要——后面的规则可以覆盖前面的规则，无论它是 `allow` 还是 `deny`。

## 规则分类

playpen 将用户的 filesystem 配置解析为三类规则，每类再按路径特征细分：

| 类别 | 路径类型 | 示例 | 说明 |
|------|---------|------|------|
| writable root | 绝对/相对路径（目录） | `rw .`、`rw /tmp/project/` | 解析为绝对路径，约束为 `subpath` 或 `regex` |
| writable pattern | 文件名/目录模式 | `rw .cargo/`、`rw *.txt` | 使用 regex 匹配任意位置 |
| readonly root | 绝对路径（目录） | `r- ~/`、`r- /etc/` | 以 `/` 结尾 → **宽泛** |
| readonly root | 绝对路径（文件） | `r- /etc/ssl/cert.pem` | 不以 `/` 结尾 → **精确** |
| readonly pattern | 文件名/目录模式 | `r- readonly.txt`、`r- Library/` | 使用 regex |
| deny pattern | 绝对路径 | `-- /tmp/secret` | 解析为绝对路径 |
| deny pattern | 文件名/目录模式 | `-- .env`、`-- *.pem`、`-- .ssh/` | 使用 regex |

**关键区分**：以 `/` 结尾的路径是"目录树"（范围大），不以 `/` 结尾的路径是"具体文件"（范围小）。输出顺序依赖此区分来正确处理 last-match 覆盖关系。

## 核心策略：`require-not` 嵌入

deny 模式通过 `(require-all (require-not ...))` **嵌入到 allow 规则内部**，而不是生成独立 `(deny ...)` 规则。这确保 deny 的作用域严格限定在对应 allow 规则的范围内。

### writable allow 中嵌入 deny

`rw .` + `-- .env` 生成：

```sbpl
(allow file-read* file-write*
  (require-all
    (regex #"^/project/.*")
    (require-not (regex #"\.env$"))
    (require-not (regex #"\.pem$"))
  )
)
```

写操作：`require-not` 拒绝 `file-write*`，`.env` 不可写。
读操作：如果存在宽泛的 readonly allow（如 `r- ~/`），可能导致 `.env` 仍可读——因此需要在 readonly 中也嵌入 deny（见下节）。

> 在 `require-all` 内部，deny regex **去掉 `(^|/)` 前缀**，因为外层已有 `(subpath "/root/")` 或 `(regex #"^/root/.*")` 约束了路径前缀。只需匹配文件名部分即可。

### 宽泛 readonly allow read 嵌入 deny

`r- ~/` + `-- .env` 生成：

```sbpl
(allow file-read*
  (require-all
    (subpath "/Users/morlay/")
    (require-not (regex #"\.env$"))
  )
)
```

这确保即使宽泛的 `r- ~/` 覆盖了整个 HOME 目录，`.env` 在 HOME 下任何位置仍不可读。

### 精确 readonly allow read 不嵌入 deny

`r- /project/cert.pem` + `-- *.pem` 生成：

```sbpl
;; 精确 readonly allow read 在最后，不嵌入 deny
(allow file-read* (regex #"^/project/cert\.pem$"))
```

不嵌入 deny 是为了让精确路径的 `r-` 可以覆盖通配的 `-- *.pem`——典型的"例外"场景。反过来说，如果是另一种情况比如 `r- /tmp/projec/subdir/` 这种，因为它是宽泛的（以 `/` 结尾），所以会嵌入 deny。

## 输出顺序

seatbelt 是 **last-match**，因此规则输出顺序直接影响最终行为。playpen 的生成顺序如下：

```
1. 宽泛 readonly allow read（以 / 结尾的 roots + patterns）
   ── 嵌入 deny require-not

2. 宽泛 readonly deny write（仅以 / 结尾的 roots）
   ── 在 writable 之前，让 writable allow 能覆盖

3. writable roots allow
   ── 嵌入 deny require-not

4. writable patterns allow
   ── 嵌入 deny require-not

5. 精确 readonly deny write（不以 / 结尾的 roots + ALL patterns）
   ── 在 writable 之后，能覆盖 writable allow

6. 精确 readonly allow read（不以 / 结尾的 roots）
   ── 不嵌入 deny，在最后输出
```

### 为什么宽泛 deny write 在 writable 之前？

`r- ~/` 的 deny write 是 `(deny file-write* (subpath "/home/"))`，如果放在 writable 之后，它会覆盖 `rw .cargo/` 的 allow。放在 writable 之前，writable allow（在后）可以覆盖它。

```
;; 正确
(deny file-write* (subpath "/home/"))           ;; r- ~/   — 先 deny
(allow file-read* file-write* (regex #"\.cargo")) ;; rw .cargo/ — 后 allow 覆盖 ✓
```

### 为什么精确 deny write 在 writable 之后？

`r- readonly.txt` 的 deny write 如果放在 writable 之前，会被 `rw .` 的 writable allow 覆盖。放在 writable 之后，精确 deny 能覆盖 writable allow。

```
;; 正确
(allow file-read* file-write* (subpath "/project/")) ;; rw .
(deny file-write* (regex #"(^|/)readonly\.txt$"))   ;; r- readonly.txt — 后 deny 覆盖 ✓
```

## 实例分析

假设配置：

```toml
[filesystem]
access = """
rw .
rw .cargo/
r- ~/
r- /etc/ssl/cert.pem
-- .env
-- *.pem
"""
```

生成的 profile 关键部分：

```sbpl
;; 1. 宽泛 readonly allow read（嵌入 deny）
(allow file-read*
  (require-all
    (subpath "/Users/morlay/")
    (require-not (regex #"\.env$"))
    (require-not (regex #"\.pem$"))
  ))

;; 2. 宽泛 readonly deny write
(deny file-write* (subpath "/Users/morlay/"))

;; 3. writable root allow（嵌入 deny）
(allow file-read* file-write*
  (require-all
    (regex #"^/project/.*")
    (require-not (regex #"\.env$"))
    (require-not (regex #"\.pem$"))
  ))

;; 4. writable pattern allow（嵌入 deny）
(allow file-read* file-write*
  (require-all
    (regex #"(^|/)\.cargo/?")
    (require-not (regex #"(^|/)\.env$"))
    (require-not (regex #"(^|/)\.pem$"))
  ))

;; 5. 精确 readonly deny write
(deny file-write* (literal "/etc/ssl/cert.pem"))

;; 6. 精确 readonly allow read（不嵌 deny）
(allow file-read* (regex #"^/etc/ssl/cert\.pem$"))
```

### 各路径的最终权限

| 路径 | 读 | 写 | 原因 |
|------|:--:|:--:|------|
| `/project/src/main.rs` | ✅ | ✅ | writable root allow 匹配 |
| `/project/.env` | ❌ | ❌ | writable root 中 require-not 拒绝 |
| `/project/cert.pem` | ❌ | ❌ | writable root 中 require-not 拒绝 |
| `/home/user/notes.txt` | ✅ | ❌ | 宽泛 readonly allow read 匹配；宽泛 deny write 拒绝 |
| `/home/user/.env` | ❌ | ❌ | 宽泛 readonly 中 require-not 拒绝 |
| `/home/.cargo/bin/cargo` | ✅ | ✅ | writable pattern 覆盖宽泛 deny write；但不受 .env/.pem deny 影响 |
| `/etc/ssl/cert.pem` | ✅ | ❌ | 精确 readonly allow read 允许读；精确 deny write 拒绝写 |
| `/tmp/sandbox_test/.env` | ✅ | ✅ | 不在 writable/readonly 区域内，由 platform defaults 的 `/tmp` 规则放行 |

## writable patterns 中 deny 嵌入的考量

writable patterns（如 `rw .cargo/`）使用 regex 匹配任意位置的路径。因此对其嵌入的 deny regex **保留 `(^|/)` 前缀**，确保在路径组件边界处精确匹配：

```sbpl
;; .cargo/ 不会误匹配 xcargo/（因为有 (^|/) 前缀）
(require-not (regex #"(^|/)\.env$"))
```

而对于 writable root 中嵌入的 deny，由于外层已有 `(subpath "/root/")` 或 `(regex #"^/root/.*")` 约束，去掉 `(^|/)` 前缀即可。

## 局限与注意事项

1. **platform defaults 中的 `/tmp` 规则**：`(allow file-read* file-write* (subpath "/tmp"))` 在 base policy 之后、用户规则之前输出。如果 writable root 也在 `/tmp` 下，用户规则（在后）能覆盖 platform defaults。但如果 deny 嵌入在 allow 中、readonly allow 又在前面且范围更广，则可能被提前放行。测试时应使用非 `/tmp` 路径。

2. **嵌套 sandbox**：在 sandbox 内再调用 `sandbox-exec` 无意义。集成测试通过 `PLAYPEN_SANDBOXED` 环境变量检测并跳过。

3. **xcrun / Xcode 临时文件**：macOS 构建工具链会在 `$TMPDIR`（通常指向 `/var/folders/...`）创建缓存文件，该路径不在 writable/readonly 区域内。playpen 执行时强制设置 `TMPDIR=/tmp` 来解决。

4. **seatbelt 的 regex 语法**：在 `require-not` 中使用 `(^|/)` 前缀时，格式为 `(regex #"(^|/)\.env$")`。括号在 `#"..."` 内视为正则表达式的一部分，不会被 SBPL 解析器误解析。
