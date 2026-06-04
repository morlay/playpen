use std::path::Path;

use super::*;

#[test]
fn workspace_check_path_allowed() {
    let tmp = tempfile::tempdir().unwrap();
    let rules = vec![];
    let ws = Workspace::new(
        tmp.path().to_path_buf(),
        std::sync::Arc::new(sandbox::SandboxConfig {
            shell_bin: "/bin/zsh".into(),
            policy_class: Default::default(),
            exec_policy: Default::default(),
            allow_pipe: false,
            allow_multiple: false,
        }),
        rules,
    );
    // 空规则默认拒绝
    let result = ws.check_path(Path::new("/etc/hosts"));
    assert!(matches!(result, sandbox::config::ValidationResult::Denied));
}

#[test]
fn workspace_read_write() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("test.txt");
    let rules = vec![sandbox::config::ParsedRule {
        raw: "rw .".into(),
        prefix: sandbox::config::RulePrefix::Allow,
        pattern: tmp.path().to_string_lossy().to_string(),
    }];
    let ws = Workspace::new(
        tmp.path().to_path_buf(),
        std::sync::Arc::new(sandbox::SandboxConfig {
            shell_bin: "/bin/zsh".into(),
            policy_class: Default::default(),
            exec_policy: Default::default(),
            allow_pipe: false,
            allow_multiple: false,
        }),
        rules,
    );

    // 写入
    ws.write_file(&file_path, "hello").unwrap();
    // 读取
    let content = ws.read_file(&file_path).unwrap();
    assert_eq!(content, "hello");
}
