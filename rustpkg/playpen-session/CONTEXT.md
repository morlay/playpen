# 持久化（Session）

Session 与事件的持久化存储，支持 rewind / replay。基于 sea-orm + SQLite 实现。

## 术语

**SessionService**：
session CRUD trait。提供 `create()` / `get(id)` / `rewind(event_id)` / `delete(id)` / `list(limit, offset)`，均返回 `anyhow::Result`。
_避免使用_：repository、DAO

**Session**：
session 只读视图 trait。提供 `id()` / `state()` / `events()`，分别返回 id、`State` 视图、`Events` 视图。具体实现实时查询 DB。

**Events**：
session 事件序列 trait（按追加顺序）。提供 `all()`（流式遍历）、`len()` / `is_empty()`、`append(event)`（追加事件并回填 store 分配的 id）、`by_role(roles)`（按角色过滤的 builder 视图）。
_避免使用_：消息列表、日志

**State**：
键值状态 trait，value 为 JSON。提供 `get(key)`（取最新值）与 `entities()`（流式遍历所有键值）。状态写入通过 `StateUpdate` 事件完成，无 `set` 方法。

**Role**：
事件角色分类枚举。变体 `User` / `Model` / `Function` / `Turn` / `State`，对应 DB `events.role` 列；`as_str()` 返回小写字符串（`user` / `model` / `function` / `turn` / `state`）。

**SessionStats**：
session 级统计。字段 `token_usage`（累加各回合 `TurnStop` 的 token 用量）、`tool_calls`（工具调用总次数与按名称计数）、`turns`（回合数）。由 `from_events(events)` 从事件列表计算。

**DBSessionService**：
`SessionService` 的 sea-orm + SQLite 实现。通过 `new(db)` 构造，`migrate()` 开启 SQLite WAL 并运行迁移。事件以 zstd+json 压缩存入 `events` 表，session 与事件的关联记录在 `session_events` 表（含 sequence）。`rewind(event_id)` 按 event_id 定位 sequence，删除该事件及其后所有事件，并将 head 回退到前一事件。
