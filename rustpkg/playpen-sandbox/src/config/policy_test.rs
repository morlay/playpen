use super::ShellPolicy;

#[test]
fn shell_policy_empty_allows_all() {
    let policy = ShellPolicy::from_raw("");
    assert!(policy.is_empty());
    assert!(policy.check(&["ls".to_string()]).is_none());
    assert!(policy.is_empty());
}

#[test]
fn shell_policy_allow_echo() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
cat *"#,
    );
    let result = policy.check(&["echo".to_string(), "hello".to_string()]);
    assert!(result.is_some_and(|(_, allowed)| allowed));
}

#[test]
fn shell_policy_no_match_is_none() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
cat *"#,
    );
    let result = policy.check(&["ls".to_string()]);
    assert!(result.is_none());
}

#[test]
fn shell_policy_deny_specific() {
    let policy = ShellPolicy::from_raw(
        r#"echo *
!echo danger *"#,
    );
    assert!(
        policy
            .check(&["echo".to_string(), "hello".to_string()])
            .is_some_and(|(_, allowed)| allowed)
    );
    assert!(
        policy
            .check(&[
                "echo".to_string(),
                "danger".to_string(),
                "alert".to_string()
            ])
            .is_some_and(|(_, allowed)| !allowed)
    );
}

#[test]
fn flag_order_independent() {
    let policy = ShellPolicy::from_raw("git push *");
    assert!(
        policy
            .check(&[
                "git".to_string(),
                "--no-pager".to_string(),
                "push".to_string(),
                "origin".to_string()
            ])
            .is_some_and(|(_, allowed)| allowed)
    );
}

#[test]
fn deny_flag_order_independent() {
    let policy = ShellPolicy::from_raw(
        r#"git *
!git push *"#,
    );
    assert!(
        policy
            .check(&[
                "git".to_string(),
                "--no-pager".to_string(),
                "push".to_string(),
                "origin".to_string()
            ])
            .is_some_and(|(_, allowed)| !allowed)
    );
    assert!(
        policy
            .check(&["git".to_string(), "status".to_string()])
            .is_some_and(|(_, allowed)| allowed)
    );
}

#[test]
fn docker_flag_variation() {
    let policy = ShellPolicy::from_raw("docker build *");
    assert!(
        policy
            .check(&[
                "docker".to_string(),
                "-f".to_string(),
                "build".to_string(),
                "-b".to_string()
            ])
            .is_some_and(|(_, allowed)| allowed)
    );
}

#[test]
fn subcommand_order_matters() {
    let policy = ShellPolicy::from_raw("git push *");
    assert!(
        policy
            .check(&["git".to_string(), "push".to_string(), "origin".to_string()])
            .is_some_and(|(_, allowed)| allowed)
    );
    assert!(
        policy
            .check(&["git".to_string(), "pull".to_string()])
            .is_none()
    );
}
