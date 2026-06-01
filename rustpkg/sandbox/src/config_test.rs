use super::{
    RulePrefix, ValidationResult, parse_filesystem_string, parse_network_string, simple_glob_match,
    validate_filesystem_path, validate_network_domain,
};
use std::path::Path;

// ---- 前缀解析：filesystem ----

#[test]
fn parse_empty_string() {
    assert!(parse_filesystem_string("").is_empty());
}

#[test]
fn parse_fs_ignores_comments_and_blanks() {
    let raw = "\n# comment\nrw .cache/\n# another\n\n-- .env\n";
    let rules = parse_filesystem_string(raw);
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].raw, "rw .cache/");
    assert_eq!(rules[1].raw, "-- .env");
}

#[test]
fn parse_fs_allow_no_prefix() {
    let rules = parse_filesystem_string("src/");
    assert_eq!(rules[0].prefix, RulePrefix::Allow);
    assert_eq!(rules[0].pattern, "src/");
}

#[test]
fn parse_fs_rw() {
    let rules = parse_filesystem_string("rw .cache/");
    assert_eq!(rules[0].prefix, RulePrefix::Allow);
    assert_eq!(rules[0].pattern, ".cache/");
}

#[test]
fn parse_fs_readonly_r_minus() {
    let rules = parse_filesystem_string("r- /etc/");
    assert_eq!(rules[0].prefix, RulePrefix::ReadOnly);
    assert_eq!(rules[0].pattern, "/etc/");
}

#[test]
fn parse_fs_deny_double_dash() {
    let rules = parse_filesystem_string("-- .env");
    assert_eq!(rules[0].prefix, RulePrefix::Deny);
    assert_eq!(rules[0].pattern, ".env");
}

#[test]
fn parse_fs_mixed_prefixes() {
    let raw = "rw .cache/\nr- /etc/\n-- .env";
    let rules = parse_filesystem_string(raw);
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].prefix, RulePrefix::Allow);
    assert_eq!(rules[0].pattern, ".cache/");
    assert_eq!(rules[1].prefix, RulePrefix::ReadOnly);
    assert_eq!(rules[1].pattern, "/etc/");
    assert_eq!(rules[2].prefix, RulePrefix::Deny);
    assert_eq!(rules[2].pattern, ".env");
}

// ---- 前缀解析：network ----

#[test]
fn parse_net_empty() {
    assert!(parse_network_string("").is_empty());
}

#[test]
fn parse_net_allow_no_prefix() {
    let rules = parse_network_string("api.example.com");
    assert_eq!(rules[0].prefix, RulePrefix::Allow);
    assert_eq!(rules[0].pattern, "api.example.com");
}

#[test]
fn parse_net_bang() {
    let rules = parse_network_string("!api.example.com");
    assert_eq!(rules[0].prefix, RulePrefix::Deny);
    assert_eq!(rules[0].pattern, "api.example.com");
}

#[test]
fn parse_net_mixed() {
    let raw = "*.example.com\n!api.example.com\n*.github.com";
    let rules = parse_network_string(raw);
    assert_eq!(rules.len(), 3);
    assert_eq!(rules[0].prefix, RulePrefix::Allow);
    assert_eq!(rules[0].pattern, "*.example.com");
    assert_eq!(rules[1].prefix, RulePrefix::Deny);
    assert_eq!(rules[1].pattern, "api.example.com");
    assert_eq!(rules[2].prefix, RulePrefix::Allow);
    assert_eq!(rules[2].pattern, "*.github.com");
}

// ---- 文件系统验证 ----

#[test]
fn validate_path_allow_rw() {
    let rules = parse_filesystem_string("rw .cache/");
    let cwd = Path::new("/home/user/project");
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/home/user/project/.cache/foo")),
        ValidationResult::Allowed
    );
}

#[test]
fn validate_path_default_deny_unlisted() {
    let rules = parse_filesystem_string("rw .cache/");
    let cwd = Path::new("/home/user/project");
    // .cache/ 之外的路径默认拒绝
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/home/user/project/src/main.rs")),
        ValidationResult::Denied
    );
}

#[test]
fn validate_path_readonly_r_minus() {
    let rules = parse_filesystem_string("r- /etc/");
    let cwd = Path::new("/home/user/project");
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/etc/passwd")),
        ValidationResult::ReadOnly
    );
}

#[test]
fn validate_path_deny_double_dash() {
    let rules = parse_filesystem_string("-- .env");
    let cwd = Path::new("/home/user/project");
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/home/user/project/.env")),
        ValidationResult::Denied
    );
}

#[test]
fn validate_path_deny_wildcard_ext() {
    let rules = parse_filesystem_string("-- *.pem");
    let cwd = Path::new("/home/user/project");
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/home/user/project/cert.pem")),
        ValidationResult::Denied
    );
}

#[test]
fn validate_path_no_rules() {
    let rules = parse_filesystem_string("");
    let cwd = Path::new("/home/user/project");
    assert_eq!(
        validate_filesystem_path(&rules, cwd, Path::new("/anything")),
        ValidationResult::Denied
    );
}

// ---- 网络验证 ----

#[test]
fn validate_domain_allow_specific() {
    let rules = parse_network_string("api.example.com");
    assert_eq!(
        validate_network_domain(&rules, "api.example.com"),
        ValidationResult::Allowed
    );
}

#[test]
fn validate_domain_deny_unlisted() {
    let rules = parse_network_string("api.example.com");
    // 未列出的域名默认拒绝
    assert_eq!(
        validate_network_domain(&rules, "google.com"),
        ValidationResult::Denied
    );
}

#[test]
fn validate_domain_allow_wildcard() {
    let rules = parse_network_string("*.example.com");
    assert_eq!(
        validate_network_domain(&rules, "api.example.com"),
        ValidationResult::Allowed
    );
    assert_eq!(
        validate_network_domain(&rules, "web.example.com"),
        ValidationResult::Allowed
    );
}

#[test]
fn validate_domain_deny_bang() {
    // 拒绝规则在前，精确匹配优先于通配
    let rules = parse_network_string("!api.example.com\n*.example.com");
    assert_eq!(
        validate_network_domain(&rules, "api.example.com"),
        ValidationResult::Denied
    );
    assert_eq!(
        validate_network_domain(&rules, "web.example.com"),
        ValidationResult::Allowed
    );
}

#[test]
fn validate_domain_no_rules() {
    let rules = parse_network_string("");
    assert_eq!(
        validate_network_domain(&rules, "anything.com"),
        ValidationResult::Denied
    );
}

// ---- glob 匹配 ----

#[test]
fn simple_glob_match_exact() {
    assert!(simple_glob_match("hello", "hello"));
    assert!(!simple_glob_match("hello", "world"));
}

#[test]
fn simple_glob_match_wildcard() {
    assert!(simple_glob_match("*.pem", "cert.pem"));
    assert!(!simple_glob_match("*.pem", "cert.pub"));
    assert!(simple_glob_match(".env.*", ".env.prod"));
    assert!(simple_glob_match("*", "anything"));
}
