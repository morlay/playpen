use super::ShellPolicy;

#[test]
fn shell_policy_empty_allows_all() {
    let policy = ShellPolicy::from_raw("");
    assert!(policy.is_empty());
    assert_eq!(policy.check(&["ls".to_string()]), Some(true));
}

#[test]
fn shell_policy_allow_echo() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
cat *"#,
    );
    let result = policy.check(&["echo".to_string(), "hello".to_string()]);
    assert_eq!(result, Some(true));
}

#[test]
fn shell_policy_no_match_is_none() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
cat *"#,
    );
    let result = policy.check(&["ls".to_string()]);
    assert_eq!(result, None);
}

#[test]
fn shell_policy_deny_specific() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
!echo danger *"#,
    );
    assert_eq!(
        policy.check(&["echo".to_string(), "hello".to_string()]),
        Some(true)
    );
    assert_eq!(
        policy.check(&[
            "echo".to_string(),
            "danger".to_string(),
            "alert".to_string()
        ]),
        Some(false)
    );
}

#[test]
fn flag_order_independent() {
    let policy = ShellPolicy::from_raw("git push *");
    assert_eq!(
        policy.check(&[
            "git".to_string(),
            "--no-pager".to_string(),
            "push".to_string(),
            "origin".to_string()
        ]),
        Some(true)
    );
}

#[test]
fn deny_flag_order_independent() {
    let policy = ShellPolicy::from_raw(
        r#"git *
!git push *"#,
    );
    assert_eq!(
        policy.check(&[
            "git".to_string(),
            "--no-pager".to_string(),
            "push".to_string(),
            "origin".to_string()
        ]),
        Some(false)
    );
    assert_eq!(
        policy.check(&["git".to_string(), "status".to_string()]),
        Some(true)
    );
}

#[test]
fn docker_flag_variation() {
    let policy = ShellPolicy::from_raw("docker build *");
    assert_eq!(
        policy.check(&[
            "docker".to_string(),
            "-f".to_string(),
            "build".to_string(),
            "-b".to_string()
        ]),
        Some(true)
    );
}

#[test]
fn subcommand_order_matters() {
    let policy = ShellPolicy::from_raw("git push *");
    assert_eq!(
        policy.check(&["git".to_string(), "push".to_string(), "origin".to_string()]),
        Some(true)
    );
    assert_eq!(policy.check(&["git".to_string(), "pull".to_string()]), None);
}
