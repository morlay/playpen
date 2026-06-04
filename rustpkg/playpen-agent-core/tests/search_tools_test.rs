use std::sync::Arc;

use playpen_agent_core::tools::Workspace;
use playpen_agent_core::tools::find::FindRigTool;
use playpen_agent_core::tools::grep::GrepRigTool;
use rig_core::tool::Tool;
use sandbox::config::ParsedRule;
use sandbox::config::RulePrefix;

fn test_workspace(dir: &std::path::Path) -> Arc<Workspace> {
    let rules = vec![ParsedRule {
        raw: "rw .".into(),
        prefix: RulePrefix::Allow,
        pattern: dir.to_string_lossy().to_string(),
    }];
    Arc::new(Workspace::new(
        dir.to_path_buf(),
        Arc::new(sandbox::SandboxConfig {
            shell_bin: "/bin/zsh".into(),
            policy_class: Default::default(),
            exec_policy: Default::default(),
            allow_pipe: false,
            allow_multiple: false,
        }),
        rules,
    ))
}

#[test]
fn find_files_by_pattern() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "").unwrap();
    std::fs::write(tmp.path().join("b.rs"), "").unwrap();
    std::fs::write(tmp.path().join("c.txt"), "").unwrap();
    std::fs::create_dir(tmp.path().join("sub")).unwrap();
    std::fs::write(tmp.path().join("sub/d.rs"), "").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = FindRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "find");

    let result = rt.block_on(tool.call(playpen_agent_core::tools::find::FindParams {
        pattern: "*.rs".into(),
        path: Some(tmp.path().to_string_lossy().to_string()),
        limit: None,
    })).unwrap();
    assert!(result.contains("a.rs"));
    assert!(result.contains("b.rs"));
    assert!(result.contains("d.rs"));
    // 不包含 .txt
    assert!(!result.contains("c.txt"));
}

#[test]
fn grep_search_content() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("main.rs"), "fn main() {\n    println!(\"hello\");\n}\n").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = GrepRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "grep");

    let result = rt.block_on(tool.call(playpen_agent_core::tools::grep::GrepParams {
        pattern: "println".into(),
        path: Some(tmp.path().join("main.rs").to_string_lossy().to_string()),
        glob: None,
        ignore_case: None,
    })).unwrap();
    assert!(result.contains("println"));
}

#[test]
fn grep_no_match() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("main.rs"), "fn main() {}\n").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = GrepRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let result = rt.block_on(tool.call(playpen_agent_core::tools::grep::GrepParams {
        pattern: "nonexistent".into(),
        path: Some(tmp.path().to_string_lossy().to_string()),
        glob: None,
        ignore_case: None,
    })).unwrap();
    assert!(result.contains("共 0 个匹配"));
}
