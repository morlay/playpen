use super::generate_profile;
use crate::config::policy::PolicyClassification;

fn policy(writable: &[&str], deny: &[&str], readonly: &[&str]) -> PolicyClassification {
    PolicyClassification {
        writable_roots: writable.iter().map(|s| s.to_string()).collect(),
        writable_patterns: Vec::new(),
        deny_patterns: deny.iter().map(|s| s.to_string()).collect(),
        readonly_roots: readonly.iter().map(|s| s.to_string()).collect(),
        readonly_patterns: Vec::new(),
    }
}

#[test]
fn profile_has_base_rules() {
    let p = generate_profile(&policy(&[], &[], &[]));
    assert!(p.contains("(deny default)"));
    assert!(p.contains("(allow process-exec)"));
}

#[test]
fn profile_scoped_deny_in_writable_roots() {
    let p = generate_profile(&policy(
        &["/Users/morlay/src/github.com/morlay/playpen/rustpkg/sandbox/target/test"],
        &[".ssh/", ".env"],
        &["/usr/"],
    ));

    assert!(p.contains("require-not"));
    assert!(p.contains("require-all"));
    assert!(p.contains("(require-not (regex #\"\\.ssh/?\"))"));
    assert!(p.contains("(require-not (regex #\"\\.env$\"))"));

    assert!(p.contains("(allow file-read* file-write*"));

    assert!(p.contains("(deny file-write* (subpath \"/usr/\"))"));

    assert!(!p.contains("(deny file-read*"));
}

#[test]
fn profile_readonly_wide_has_require_not() {
    let p = generate_profile(&policy(&[], &[".env"], &["/Users/morlay/"]));
    assert!(p.contains("require-not"));

    assert!(p.contains("(require-not (regex #\"\\.env$\"))"));
    assert!(!p.contains("(deny file-read*"));
}

#[test]
fn profile_readonly_exact_no_require_not() {
    let p = generate_profile(&policy(&["/project/"], &["*.pem"], &["/project/cert.pem"]));

    let pos_exact = p.rfind("; readonly（allow read，精确路径）").unwrap();
    let pos_deny = p.rfind("(require-not");

    let after_exact = &p[pos_exact..];
    assert!(!after_exact.contains("require-not"));

    if let Some(pos_deny_val) = pos_deny {
        assert!(pos_deny_val < pos_exact);
    }
}
