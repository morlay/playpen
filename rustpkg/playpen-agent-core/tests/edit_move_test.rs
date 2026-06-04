use std::sync::Arc;

use playpen_agent_core::tools::Workspace;
use playpen_agent_core::tools::r#move::MoveRigTool;
use playpen_agent_core::tools::edit::EditRigTool;
use playpen_agent_core::tools::edit::{EditOperation, EditParams};
use playpen_agent_core::tools::r#move::MoveParams;
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
fn edit_file_exact_match() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("edit.txt");
    std::fs::write(&file, "hello world\nfoo bar\n").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = EditRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "edit");

    let result = rt.block_on(tool.call(EditParams {
        path: file.to_string_lossy().to_string(),
        edits: vec![EditOperation { old_text: "foo bar".into(), new_text: "baz qux".into() }],
    })).unwrap();
    assert!(result.contains("成功"));

    let content = std::fs::read_to_string(&file).unwrap();
    assert_eq!(content, "hello world\nbaz qux\n");
}

#[test]
fn move_file_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src.txt");
    let dst = tmp.path().join("dst.txt");
    std::fs::write(&src, "data").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = MoveRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "move");

    let result = rt.block_on(tool.call(MoveParams {
        source: src.to_string_lossy().to_string(),
        destination: dst.to_string_lossy().to_string(),
    })).unwrap();
    assert!(result.contains("已移动"));

    assert!(!src.exists());
    assert_eq!(std::fs::read_to_string(&dst).unwrap(), "data");
}

#[test]
fn move_file_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("to_delete.txt");
    std::fs::write(&src, "data").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = MoveRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let result = rt.block_on(tool.call(MoveParams {
        source: src.to_string_lossy().to_string(),
        destination: "/dev/null".into(),
    })).unwrap();
    assert!(result.contains("已删除"));
    assert!(!src.exists());
}
