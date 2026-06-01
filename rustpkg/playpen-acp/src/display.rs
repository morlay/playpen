use std::path::Path;

use agent_client_protocol::schema::v1::{Meta, ToolKind};

pub const TOOL_NAME_META_KEY: &str = "tool_name";

pub fn meta_with_tool_name(name: &str) -> Meta {
    Meta::from_iter([(TOOL_NAME_META_KEY.into(), name.into())])
}

pub fn map_tool_kind(name: &str) -> ToolKind {
    match name {
        "read" => ToolKind::Read,
        "grep" | "find" => ToolKind::Search,
        "edit" | "write" => ToolKind::Edit,
        "move" => ToolKind::Move,
        "webfetch" => ToolKind::Fetch,
        "bash" => ToolKind::Execute,
        _ => ToolKind::Other,
    }
}

pub fn display_path(path_str: &str, project_root: &Path) -> String {
    let p = Path::new(path_str);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_root.join(p)
    };
    match abs.strip_prefix(project_root) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => abs.display().to_string(),
    }
}

pub fn build_tool_title(
    name: &str,
    arguments: &serde_json::Map<String, serde_json::Value>,
    project_root: &Path,
) -> String {
    let pick = |k: &str| {
        arguments
            .get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };

    let display_name = capitalize_first(name);

    match name {
        "read" => {
            let base = pick("path").map(|p| display_path(&p, project_root).to_string());
            let suffix = {
                let offset = arguments.get("offset").and_then(|v| v.as_u64());
                let limit = arguments.get("limit").and_then(|v| v.as_u64());
                match (offset, limit) {
                    (Some(o), Some(l)) => format!("#L{}-{}", o, o + l - 1),
                    (Some(o), None) => format!("#L{}-", o),
                    (None, Some(l)) => format!("#L1-{}", l),
                    (None, None) => String::new(),
                }
            };
            base.map(|b| format!("{display_name} [{b}]({b}){suffix}"))
        }
        "edit" | "write" => {
            pick("path").map(|p| format!("{display_name} <{}>", display_path(&p, project_root)))
        }
        "move" => {
            pick("old_path").map(|p| format!("{display_name} <{}>", display_path(&p, project_root)))
        }
        "grep" | "find" => pick("pattern").map(|p| format!("{display_name} `{p}`")),
        "bash" => pick("command"),
        "webfetch" => pick("url").map(|p| format!("{display_name} `{p}`")),
        _ => None,
    }
    .unwrap_or_else(|| display_name.to_string())
}

pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

pub fn extract_cwd(
    arguments: &serde_json::Map<String, serde_json::Value>,
    project_root: &Path,
) -> String {
    arguments
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(|p| {
            let path = Path::new(p);
            if path.is_absolute() {
                path.display().to_string()
            } else {
                project_root.join(path).display().to_string()
            }
        })
        .unwrap_or_else(|| project_root.display().to_string())
}

#[cfg(test)]
#[path = "display_test.rs"]
mod tests;
