use super::{ShellConfig, check_shell, join_args, parse_shell};
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

// ---- join_args 测试 ----

#[test]
fn join_plain_args() {
    let args = vec!["echo".into(), "hello".into()];
    let cmd = join_args(&args).unwrap();
    assert_eq!(cmd, "echo hello");
}

#[test]
fn join_ampersand_in_arg() {
    // & 在参数中应被转义，避免被 shell 解释为后台运行
    let args = vec!["rg".into(), "&thought_level".into(), "file.rs".into()];
    let cmd = join_args(&args).unwrap();
    // shlex::try_join 会用单引号包裹含特殊字符的参数
    assert_eq!(cmd, "rg '&thought_level' file.rs");
}

#[test]
fn join_arg_with_spaces() {
    let args = vec!["echo".into(), "hello world".into()];
    let cmd = join_args(&args).unwrap();
    assert_eq!(cmd, "echo 'hello world'");
}

#[test]
fn join_empty_args() {
    let args: Vec<String> = vec![];
    let cmd = join_args(&args).unwrap();
    assert_eq!(cmd, "");
}

#[test]
fn join_single_arg() {
    let args = vec!["ls".into()];
    let cmd = join_args(&args).unwrap();
    assert_eq!(cmd, "ls");
}

#[test]
fn join_special_chars_quoted() {
    let args = vec!["grep".into(), "$".into(), "file".into()];
    let cmd = join_args(&args).unwrap();
    assert_eq!(cmd, "grep '$' file");
}

// ── 沙箱 shell 规则：allow 配置有无的对比测试 ──

#[test]
fn no_allow_rules_allows_any_command() {
    // 空规则默认允许所有命令
    let policy = make_policy(&[]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("ls", &config, &policy);
    assert!(result.allowed, "空 allow 列表应允许任意命令");
}

#[test]
fn allow_ls_wildcard_permits_ls() {
    let policy = make_policy(&["ls *"]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("ls", &config, &policy);
    assert!(result.allowed, "ls 应被 ls * 规则允许");
}

#[test]
fn allow_ls_wildcard_permits_ls_with_args() {
    let policy = make_policy(&["ls *"]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("ls -la /tmp", &config, &policy);
    assert!(result.allowed, "ls -la /tmp 应被 ls * 规则允许");
}

#[test]
fn allow_ls_only_denies_other_commands() {
    let policy = make_policy(&["ls *"]);
    let config = ShellConfig {
        allow_pipe: true,
        allow_multiple: false,
    };
    let result = check_shell("cat file", &config, &policy);
    assert!(!result.allowed, "cat 不在 allow 列表中，应被拒绝");
}
