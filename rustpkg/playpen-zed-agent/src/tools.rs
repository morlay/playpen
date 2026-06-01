/// zed 工具定义
pub struct ToolDef {
    pub name: &'static str,
    /// profile 中是否启用
    pub enabled: bool,
    /// 工具分类（决定如何映射权限）
    pub category: ToolCategory,
    /// 无配置时的默认权限（deny / allow / None）
    pub default_permission: Option<&'static str>,
    /// 提示词说明（启用时生效）
    pub prompt_guideline: Option<&'static str>,
    /// 禁用时的替代方案说明
    pub prompt_guideline_disabled: Option<&'static str>,
}

/// 工具分类，对应 playpen 的权限域
pub enum ToolCategory {
    /// 文件系统只读
    FilesystemRead,
    /// 文件系统写入（含删除/移动）
    FilesystemWrite,
    /// 网络
    Network,
    /// 终端 → 固定委托给 playpen
    Shell,
    /// 不受影响
    Other,
}

/// 所有 zed 工具定义
pub const TOOLS: &[ToolDef] = &[
    // ---- 文件读取（禁用，走 shell + seatbelt 更可靠）----
    ToolDef {
        name: "grep",
        enabled: false,
        category: ToolCategory::FilesystemRead,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: Some(
            r"
搜索文件内容用 `rg ...`
",
        ),
    },
    ToolDef {
        name: "find_path",
        enabled: false,
        category: ToolCategory::FilesystemRead,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: Some(
            r"
路径搜索/目录浏览用 `fd ...`，**严禁用 `ls`、`find`**；无 `list_directory` 工具，请勿使用。
",
        ),
    },
    ToolDef {
        name: "read_file",
        enabled: true,
        category: ToolCategory::FilesystemRead,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: Some(
            r"
读取文件内容用 `bat ...`
",
        ),
    },
    // ---- 文件写入 ----
    ToolDef {
        name: "edit_file",
        enabled: true,
        category: ToolCategory::FilesystemWrite,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "write_file",
        enabled: true,
        category: ToolCategory::FilesystemWrite,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    // ---- 文件删除/移动 ----
    ToolDef {
        name: "delete_path",
        enabled: true,
        category: ToolCategory::FilesystemWrite,
        default_permission: Some("confirm"),
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    // ---- 网络 ----
    ToolDef {
        name: "fetch",
        enabled: true,
        category: ToolCategory::Network,
        default_permission: Some("allow"),
        prompt_guideline: Some(
            r"
HTTP 请求用 fetch()，**不要用 `curl` / `wget`**
",
        ),
        prompt_guideline_disabled: None,
    },
    // ---- 终端 ----
    ToolDef {
        name: "terminal",
        enabled: true,
        category: ToolCategory::Shell,
        default_permission: Some("deny"),
        prompt_guideline: Some(
            r"
**`terminal()` 必须用 `playpen` 前缀。** 示例：`playpen rg 'pattern'`、`playpen fd -t f 'name'`、`playpen eza --git-ignore --tree .`
`terminal()` 的工作目录通过 `cd` 参数指定，**不要在 command 里写 `cd`**。
默认使用遵循 `.gitignore` 的参数（如 `fd --git-ignore`），仅在需要时显式忽略 ignore 规则。
",
        ),
        prompt_guideline_disabled: None,
    },
    // ---- 其他 ----
    ToolDef {
        name: "skill",
        enabled: true,
        category: ToolCategory::Other,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "spawn_agent",
        enabled: true,
        category: ToolCategory::Other,
        default_permission: Some("allow"),
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    // ---- 禁用 ----
    ToolDef {
        name: "diagnostics",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "move_path",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "copy_path",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "list_directory",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "create_directory",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    // ---- lsp ----
    ToolDef {
        name: "apply_code_action",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "find_references",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "get_code_actions",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "go_to_definition",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    ToolDef {
        name: "rename_symbol",
        enabled: false,
        category: ToolCategory::Other,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
    // ---- 搜索 ----
    ToolDef {
        name: "search_web",
        enabled: false,
        category: ToolCategory::Network,
        default_permission: None,
        prompt_guideline: None,
        prompt_guideline_disabled: None,
    },
];
