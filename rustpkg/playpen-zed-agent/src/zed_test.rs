use super::*;
use regex::Regex;
use sandbox::config::{self, ValidationResult};
use std::path::Path;

#[test]
fn zed_regex_matches_sandbox_rules() {
    let cases: Vec<(&str, &[&str])> = vec![
        (
            "-- .env\n-- .env.*\n-- .ssh/\n-- *.pem",
            &[
                "/home/user/project/.env",
                "/home/user/project/.env.prod",
                "/home/user/project/.ssh/config",
                "/home/user/project/cert.pem",
            ],
        ),
        ("-- .env", &["/home/user/project/.env.prod"]),
        ("-- secrets/", &["/home/user/project/notsecrets/data"]),
        ("-- secrets/", &["/home/user/project/secrets/token"]),
        (
            "rw .cache/\nrw node_modules/\nrw go/pkg/",
            &[
                "/home/user/project/.cache/foo",
                "/home/user/project/node_modules/bar",
                "/home/user/project/go/pkg/mod",
            ],
        ),
    ];

    let cwd = Path::new("/home/user/project");

    for (rule_raw, paths) in &cases {
        let rules = config::parse_filesystem_string(rule_raw);
        for &path_str in *paths {
            let target = Path::new(path_str);
            let sandbox_result = config::validate_filesystem_path(&rules, cwd, target);
            let perms = generate_tool_permissions(&rules, cwd);

            let write_perm = perms.get("write_file").unwrap();
            let zed_denied =
                matches_any_pattern(path_str, write_perm.always_deny.as_deref().unwrap_or(&[]));
            let zed_allowed =
                matches_any_pattern(path_str, write_perm.always_allow.as_deref().unwrap_or(&[]));

            match sandbox_result {
                ValidationResult::Denied => assert!(
                    zed_denied || !zed_allowed,
                    "sandbox 拒绝但 zed 放行: rule={} path={}",
                    rule_raw,
                    path_str
                ),
                ValidationResult::Allowed => assert!(
                    zed_allowed || !zed_denied,
                    "sandbox 允许但 zed 拒绝: rule={} path={}",
                    rule_raw,
                    path_str
                ),
                // r- 规则不映射到 zed，由 seatbelt 兜底
                ValidationResult::ReadOnly => {}
            }
        }
    }
}

#[test]
fn test_pattern_conversion() {
    let cwd = Path::new("/home/user/project");
    assert_eq!(pattern_to_zed_regex("secrets/", cwd), "\\bsecrets/?");
    assert_eq!(pattern_to_zed_regex(".cache/", cwd), "\\.cache/?");
    assert_eq!(pattern_to_zed_regex("/etc", cwd), "^/etc");
    assert_eq!(pattern_to_zed_regex(".env", cwd), "\\.env$");
    assert_eq!(pattern_to_zed_regex("*.pem", cwd), "\\.pem$");
    assert_eq!(pattern_to_zed_regex(".env.*", cwd), "\\.env\\.[^/]*$");
}

#[test]
fn test_boundary_prevents_escape() {
    let re = Regex::new("\\bsecrets/?").unwrap();
    assert!(re.is_match("/home/user/project/secrets/token"));
    assert!(!re.is_match("/home/user/project/notsecrets/data"));

    let re_env = Regex::new("\\.env$").unwrap();
    assert!(re_env.is_match("/home/user/project/.env"));
    assert!(!re_env.is_match("/home/user/project/.env.prod"));

    let re_env_variant = Regex::new("\\.env\\.[^/]*$").unwrap();
    assert!(re_env_variant.is_match("/home/user/project/.env.prod"));
    assert!(!re_env_variant.is_match("/home/user/project/.env"));
}

#[test]
fn test_build_agent_preserves_all_existing() {
    let perms = generate_tool_permissions(&[], Path::new("/tmp"));
    let profiles = serde_json::json!({"code": {"tools": {"write_file": true}}});
    let existing = serde_json::json!({
        "custom_agent_field": true,
        "tool_permissions": {
            "custom_tp_field": 42,
            "tools": {
                "write_file": { "custom_tool_field": "keep", "default": "deny" },
                "other_tool": { "default": "allow" }
            }
        },
        "profiles": {
            "old_profile": { "custom_field": "keep", "tools": {} },
            "code": { "custom_field": "keep", "tools": { "old_tool": true } }
        }
    });

    let result = build_agent(&existing, &perms, &profiles);
    assert_eq!(result["custom_agent_field"], true);
    assert!(
        result
            .pointer("/tool_permissions/custom_tp_field")
            .is_some()
    );
    assert!(
        result
            .pointer("/tool_permissions/tools/other_tool")
            .is_some()
    );
    assert!(result.pointer("/profiles/code/custom_field").is_some());
}

fn matches_any_pattern(path: &str, patterns: &[ZedPattern]) -> bool {
    patterns.iter().any(|p| {
        if let Ok(re) = Regex::new(&p.pattern) {
            re.is_match(path)
        } else {
            false
        }
    })
}
