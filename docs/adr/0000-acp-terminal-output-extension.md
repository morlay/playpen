# ACP terminal output extension

- ACP 官方协议未定义 bash 的终端风格输出格式，Zed 通过 `ClientCapabilities._meta.terminal_output` 支持此扩展
- playpen 在 `initialize` 阶段读取该标记，后续事件映射中条件启用：
  - `terminal_output_enabled = true` 且工具名为 `bash` 时，使用三种非标准 meta 替代标准格式：`terminal_info`、`terminal_output`、`terminal_exit`
  - 非终端模式下，Zed 展示依赖 `build_tool_title()` 拼装可读标题
