use super::*;

// ── is_valid_dns_label ────────────────────────────────────────────────

#[test]
fn test_valid_dns_labels() {
    assert!(is_valid_dns_label("rewind"));
    assert!(is_valid_dns_label("code"));
    assert!(is_valid_dns_label("my-skill"));
    assert!(is_valid_dns_label("a"));
    assert!(is_valid_dns_label("z0"));
    assert!(is_valid_dns_label("1test"));
}

#[test]
fn test_invalid_dns_labels() {
    assert!(!is_valid_dns_label("")); // 空
    assert!(!is_valid_dns_label("-foo")); // 连字符开头
    assert!(!is_valid_dns_label("foo-")); // 连字符结尾
    assert!(!is_valid_dns_label("foo/bar")); // 含斜杠
    assert!(!is_valid_dns_label("foo bar")); // 含空格
    assert!(!is_valid_dns_label("foo.bar")); // 含点
    assert!(!is_valid_dns_label("foo_bar")); // 含下划线
    assert!(!is_valid_dns_label("a-b-c-")); // 连字符结尾
}

// ── parse_slash_command ──────────────────────────────────────────────

#[test]
fn test_parse_rewind() {
    let (cmd, args) = parse_slash_command("/rewind").unwrap();
    assert_eq!(cmd.kind, SlashCommandKind::Rewind);
    assert_eq!(cmd.name, "rewind");
    assert!(args.is_empty());
}

#[test]
fn test_parse_rewind_with_args() {
    let (cmd, args) = parse_slash_command("/rewind 重新分析").unwrap();
    assert_eq!(cmd.kind, SlashCommandKind::Rewind);
    assert_eq!(args, "重新分析");
}

#[test]
fn test_parse_skill() {
    let (cmd, args) = parse_slash_command("/code").unwrap();
    assert_eq!(cmd.kind, SlashCommandKind::Skill);
    assert_eq!(cmd.name, "code");
    assert!(args.is_empty());
}

#[test]
fn test_parse_skill_with_args() {
    let (cmd, args) = parse_slash_command("/code 分析 main.rs").unwrap();
    assert_eq!(cmd.kind, SlashCommandKind::Skill);
    assert_eq!(cmd.name, "code");
    assert_eq!(args, "分析 main.rs");
}

#[test]
fn test_parse_skill_with_hyphen() {
    let (cmd, args) = parse_slash_command("/my-skill 参数").unwrap();
    assert_eq!(cmd.kind, SlashCommandKind::Skill);
    assert_eq!(cmd.name, "my-skill");
    assert_eq!(args, "参数");
}

#[test]
fn test_path_like_not_matched() {
    // 路径包含斜杠，不是合法 DNS 标签
    assert!(parse_slash_command("/foo/bar").is_none());
    assert!(parse_slash_command("/a/b/c").is_none());
    assert!(parse_slash_command("/usr/local/bin").is_none());
}

#[test]
fn test_quoted_content_skipped() {
    assert!(parse_slash_command("\"/rewind\"").is_none());
    assert!(parse_slash_command("\"/code\"").is_none());
}

#[test]
fn test_code_block_skipped() {
    assert!(parse_slash_command("```/rewind```").is_none());
    assert!(parse_slash_command("```\n/code\n```").is_none());
}

#[test]
fn test_plain_text_not_matched() {
    assert!(parse_slash_command("普通文本").is_none());
    assert!(parse_slash_command("").is_none());
    assert!(parse_slash_command("/").is_none());
}

#[test]
fn test_leading_whitespace_handled() {
    let (cmd, args) = parse_slash_command("  /rewind 重新分析").unwrap();
    assert_eq!(cmd.name, "rewind");
    assert_eq!(args, "重新分析");
}

#[test]
fn test_trailing_spaces_handled() {
    let (cmd, args) = parse_slash_command("/rewind   ").unwrap();
    assert_eq!(cmd.name, "rewind");
    assert!(args.is_empty());
}

// ── build commands ────────────────────────────────────────────────────

#[test]
fn test_build_rewind_available_command() {
    let cmd = build_rewind_available_command();
    assert_eq!(cmd.name, "rewind");
    assert!(cmd.description.contains("回退"));
}

#[test]
fn test_build_skill_available_command() {
    let cmd = build_skill_available_command("code", "代码分析技能");
    assert_eq!(cmd.name, "code");
    assert_eq!(cmd.description, "代码分析技能");
}
