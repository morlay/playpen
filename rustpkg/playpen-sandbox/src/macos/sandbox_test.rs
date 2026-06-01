use super::*;
use crate::config;
use std::collections::HashMap;

fn make_sandbox(allow: &[&str], allow_pipe: bool) -> MacosSandbox {
    let mut cfg = config::Config::default();
    if !allow.is_empty() {
        cfg.shell = Some(config::ShellSection {
            allow_pipe: Some(allow_pipe),
            allow_multiple: Some(false),
            allow: allow.iter().map(|s| s.to_string()).collect(),
        });
    } else {
        cfg.shell = Some(config::ShellSection {
            allow_pipe: Some(allow_pipe),
            allow_multiple: Some(false),
            allow: vec![],
        });
    }
    MacosSandbox::new(&cfg, std::path::Path::new("/tmp"))
}

#[test]
fn wrap_command_sets_default_cwd() {
    let sb = make_sandbox(&[], true);
    let cmd = Command {
        command: "echo hello".into(),
        cwd: None,
        env: HashMap::new(),
    };
    let result = sb.wrap_command(cmd).unwrap();
    assert_eq!(result.command, "echo hello");
    assert_eq!(result.cwd.unwrap(), std::path::PathBuf::from("/tmp"));
}

#[test]
fn wrap_command_preserves_existing_cwd() {
    let sb = make_sandbox(&[], true);
    let cmd = Command {
        command: "echo hello".into(),
        cwd: Some("/custom".into()),
        env: HashMap::new(),
    };
    let result = sb.wrap_command(cmd).unwrap();
    assert_eq!(result.cwd.unwrap(), std::path::PathBuf::from("/custom"));
}

#[test]
fn wrap_command_forbidden() {
    let sb = make_sandbox(&["echo *"], false);
    let cmd = Command {
        command: "cat /etc/passwd".into(),
        cwd: None,
        env: HashMap::new(),
    };
    assert!(sb.wrap_command(cmd).is_err());
}
