use std::sync::Arc;

use playpen_agent_core::tools::{Workspace, read::ReadRigTool, write::WriteRigTool};
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
fn read_file_basic() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("hello.txt");
    std::fs::write(&file, "line1\nline2\nline3\n").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = ReadRigTool { ws };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "read");
    assert!(def.description.contains("读取"));

    let result = rt.block_on(tool.call(playpen_agent_core::tools::read::ReadParams {
        path: file.to_string_lossy().to_string(),
        offset: None,
        limit: None,
    })).unwrap();
    assert!(result.contains("line1"));
    assert!(result.contains("line2"));
    assert!(result.contains("line3"));
}

#[test]
fn read_file_with_offset() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("nums.txt");
    std::fs::write(&file, "1\n2\n3\n4\n5\n").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = ReadRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let result = rt.block_on(tool.call(playpen_agent_core::tools::read::ReadParams {
        path: file.to_string_lossy().to_string(),
        offset: Some(2),
        limit: Some(2),
    })).unwrap();
    // 显示第 2-3 行
    assert!(result.contains("2"));
    assert!(result.contains("3"));
    assert!(!result.contains("1"));
    assert!(!result.contains("4"));
}

#[test]
fn write_file_creates() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("new.txt");

    let ws = test_workspace(tmp.path());
    let tool = WriteRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    let def = rt.block_on(tool.definition(String::new()));
    assert_eq!(def.name, "write");

    let result = rt.block_on(tool.call(playpen_agent_core::tools::write::WriteParams {
        path: file.to_string_lossy().to_string(),
        content: "hello world".into(),
    })).unwrap();
    assert!(result.contains("已成功写入"));

    let content = std::fs::read_to_string(&file).unwrap();
    assert_eq!(content, "hello world");
}

#[test]
fn write_file_overwrites() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("existing.txt");
    std::fs::write(&file, "old content").unwrap();

    let ws = test_workspace(tmp.path());
    let tool = WriteRigTool { ws };
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(tool.call(playpen_agent_core::tools::write::WriteParams {
        path: file.to_string_lossy().to_string(),
        content: "new content".into(),
    })).unwrap();

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new content");
}
