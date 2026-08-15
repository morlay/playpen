# playpen-session

持久化层——Session 与事件的持久化存储，支持 rewind / replay。

## 职责

- `SessionService` trait：session CRUD（`create` / `get` / `rewind` / `delete` / `list`）
- `Session` / `Events` / `State`：session 只读视图 trait（id / state / events、事件序列、键值状态）
- `DBSessionService`：`SessionService` 的 sea-orm + SQLite 实现，事件 payload 以 zstd+json 压缩存储
- `SessionStats`：session 级统计（token 用量 / 工具调用 / turns）

## 核心模块

| 模块      | 职责                                                              |
| --------- | ----------------------------------------------------------------- |
| `events`  | `Events` trait：事件序列访问（`all` / `len` / `append` / `by_role`） |
| `role`    | `Role` 枚举：事件角色分类（`User` / `Model` / `Function` / `Turn` / `State`） |
| `service` | `SessionService` trait：session CRUD                              |
| `session` | `Session` trait：只读视图（`id` / `state` / `events`）            |
| `state`   | `State` trait：键值状态（`get` / `entities`）                     |
| `stats`   | `SessionStats`：token 用量 / 工具调用 / turns 统计                |
| `store`   | `DBSessionService`：sea-orm + SQLite 实现（含 `entities` / `migration` / `convert`） |
