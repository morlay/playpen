use super::{ShellConfig, check_shell, parse_shell};
use crate::policy::ShellPolicy;

fn make_policy(allowed: &[&str]) -> ShellPolicy {
    let raw = allowed.join("\n");
    ShellPolicy::from_raw(&raw)
}

#[test]
fn parse_simple_command() {
    let p = parse_shell("echo hello");
    assert_eq!(p.commands.len(), 1);
    assert!(!p.has_pipe);
    assert!(!p.has_multiple);
}

#[test]
fn parse_pipe_detection() {
    let p = parse_shell("cat file | grep foo");
    assert_eq!(p.commands.len(), 2);
    assert!(p.has_pipe);
}

#[test]
fn parse_multiple_detection() {
    let p = parse_shell("echo a && echo b");
    assert!(p.has_multiple);
}

#[test]
fn parse_semicolon_detection() {
    let p = parse_shell("echo a ; echo b");
    assert!(p.has_multiple);
}

#[test]
fn parse_quoted_args() {
    let p = parse_shell("echo \"hello world\" 'foo bar'");
    assert_eq!(p.commands.len(), 1);
    assert_eq!(p.commands[0].len(), 3);
    assert_eq!(p.commands[0][1], "hello world");
    assert_eq!(p.commands[0][2], "foo bar");
}

#[test]
fn check_allow_command() {
    let policy = make_policy(&["echo *"]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("echo hello", &config, &policy);
    assert!(result.allowed);
}

#[test]
fn check_deny_command() {
    let policy = make_policy(&["echo *"]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("rm -rf /", &config, &policy);
    assert!(!result.allowed);
}

#[test]
fn check_pipe_blocked() {
    let policy = make_policy(&["echo *"]);
    let config = ShellConfig {
        allow_pipe: false,
        allow_multiple: false,
    };
    let result = check_shell("echo hello | cat", &config, &policy);
    assert!(!result.allowed);
}
