use super::*;
use std::path::Path;

// ── capitalize_first ────────────────────────────────────────────────

#[test]
fn test_capitalize_first_lowercase() {
    assert_eq!(capitalize_first("hello"), "Hello");
}

#[test]
fn test_capitalize_first_already_capitalized() {
    assert_eq!(capitalize_first("Hello"), "Hello");
}

#[test]
fn test_capitalize_first_empty() {
    assert_eq!(capitalize_first(""), "");
}

#[test]
fn test_capitalize_first_single_char() {
    assert_eq!(capitalize_first("a"), "A");
}

// ── map_tool_kind ──────────────────────────────────────────────────

#[test]
fn test_map_tool_kind_read() {
    assert_eq!(map_tool_kind("read"), ToolKind::Read);
}

#[test]
fn test_map_tool_kind_edit() {
    assert_eq!(map_tool_kind("edit"), ToolKind::Edit);
    assert_eq!(map_tool_kind("write"), ToolKind::Edit);
}

#[test]
fn test_map_tool_kind_bash() {
    assert_eq!(map_tool_kind("bash"), ToolKind::Execute);
}

#[test]
fn test_map_tool_kind_unknown_defaults_to_other() {
    assert_eq!(map_tool_kind("unknown_tool"), ToolKind::Other);
}

// ── display_path ───────────────────────────────────────────────────

#[test]
fn test_display_path_relative() {
    let root = Path::new("/project");
    assert_eq!(display_path("src/main.rs", root), "src/main.rs");
}

#[test]
fn test_display_path_absolute_inside_project() {
    let root = Path::new("/project");
    assert_eq!(display_path("/project/src/main.rs", root), "src/main.rs");
}

#[test]
fn test_display_path_absolute_outside_project() {
    let root = Path::new("/project");
    let result = display_path("/outside/file.txt", root);
    assert_eq!(result, "/outside/file.txt");
}

// ── extract_cwd ────────────────────────────────────────────────────

#[test]
fn test_extract_cwd_absolute() {
    let args = serde_json::json!({"cwd": "/workspace/src"});
    let map = args.as_object().unwrap().clone();
    let result = extract_cwd(&map, Path::new("/project"));
    assert_eq!(result, "/workspace/src");
}

#[test]
fn test_extract_cwd_relative() {
    let args = serde_json::json!({"cwd": "src"});
    let map = args.as_object().unwrap().clone();
    let result = extract_cwd(&map, Path::new("/project"));
    assert_eq!(result, "/project/src");
}

#[test]
fn test_extract_cwd_missing_falls_back_to_project_root() {
    let args = serde_json::Map::new();
    let result = extract_cwd(&args, Path::new("/project"));
    assert_eq!(result, "/project");
}

// ── build_tool_title ───────────────────────────────────────────────

#[test]
fn test_build_tool_title_bash() {
    let args = serde_json::json!({"command": "echo hello"});
    let map = args.as_object().unwrap().clone();
    let title = build_tool_title("bash", &map, Path::new("/project"));
    assert_eq!(title, "echo hello");
}

#[test]
fn test_build_tool_title_bash_no_command() {
    let args = serde_json::Map::new();
    let title = build_tool_title("bash", &args, Path::new("/project"));
    assert_eq!(title, "Bash");
}

#[test]
fn test_build_tool_title_read_with_path() {
    let args = serde_json::json!({"path": "src/main.rs", "offset": 10, "limit": 20});
    let map = args.as_object().unwrap().clone();
    let title = build_tool_title("read", &map, Path::new("/project"));
    assert_eq!(title, "Read [src/main.rs](src/main.rs)#L10-29");
}

#[test]
fn test_build_tool_title_read_no_offset_limit() {
    let args = serde_json::json!({"path": "src/main.rs"});
    let map = args.as_object().unwrap().clone();
    let title = build_tool_title("read", &map, Path::new("/project"));
    assert_eq!(title, "Read [src/main.rs](src/main.rs)");
}

#[test]
fn test_build_tool_title_edit_with_path() {
    let args = serde_json::json!({"path": "src/main.rs"});
    let map = args.as_object().unwrap().clone();
    let title = build_tool_title("edit", &map, Path::new("/project"));
    assert_eq!(title, "Edit <src/main.rs>");
}

#[test]
fn test_build_tool_title_grep_with_pattern() {
    let args = serde_json::json!({"pattern": "fn main"});
    let map = args.as_object().unwrap().clone();
    let title = build_tool_title("grep", &map, Path::new("/project"));
    assert_eq!(title, "Grep `fn main`");
}

#[test]
fn test_build_tool_title_unknown() {
    let args = serde_json::Map::new();
    let title = build_tool_title("some_tool", &args, Path::new("/project"));
    assert_eq!(title, "Some_tool");
}

// ── meta_with_tool_name ────────────────────────────────────────────

#[test]
fn test_meta_with_tool_name() {
    let meta = meta_with_tool_name("bash");
    let entries: Vec<_> = meta.into_iter().collect();
    assert!(entries.contains(&("tool_name".into(), "bash".into())));
}
