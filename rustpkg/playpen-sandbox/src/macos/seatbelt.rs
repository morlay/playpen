use crate::config::policy::PolicyClassification;

const BASE_POLICY: &str = include_str!("seatbelt_base_policy.sbpl");
const NETWORK_POLICY: &str = include_str!("seatbelt_network_policy.sbpl");
const PLATFORM_DEFAULTS: &str =
    include_str!("seatbelt_restricted_read_only_platform_defaults.sbpl");

pub fn generate_profile(policy: &PolicyClassification) -> String {
    // macOS 上 /var、/tmp、/etc 是 /private/var、/private/tmp、/private/etc 的 symlink。
    // seatbelt 的 subpath/regex 规则不跟随 symlink，需要为这些路径生成双重规则。
    let writable_roots = expand_symlink_roots(&policy.writable_roots);
    let readonly_roots = expand_symlink_roots(&policy.readonly_roots);

    let mut lines = vec![
        BASE_POLICY.to_string(),
        "".into(),
        NETWORK_POLICY.to_string(),
        "".into(),
        "(allow network-outbound)".into(),
        "(allow network-inbound)".into(),
        "".into(),
        PLATFORM_DEFAULTS.to_string(),
        "".into(),
        "; === 用户文件规则 ===".into(),
        "".into(),
    ];

    let has_rules = !writable_roots.is_empty()
        || !policy.writable_patterns.is_empty()
        || !readonly_roots.is_empty()
        || !policy.readonly_patterns.is_empty();

    if !has_rules {
        return lines.join("\n");
    }

    let has_readonly = !readonly_roots.is_empty() || !policy.readonly_patterns.is_empty();

    if has_readonly {
        lines.push("; readonly（allow read）".into());
        for root in &readonly_roots {
            if root.ends_with('/') {
                let deny_clauses = build_deny_clauses_for_root(root, &policy.deny_patterns);
                if deny_clauses.is_empty() {
                    lines.push(format!("(allow file-read* (subpath \"{}\"))", root));
                } else {
                    let mut parts = vec![format!("(subpath \"{}\")", root)];
                    parts.extend(deny_clauses);
                    let body = parts.join("\n    ");
                    lines.push(format!(
                        "(allow file-read*\n  (require-all\n    {}\n  )\n)",
                        body
                    ));
                }
            }
        }
        for pattern in &policy.readonly_patterns {
            let regex = glob_to_seatbelt_regex(pattern);
            let deny_clauses = build_deny_regex_clauses_all(&policy.deny_patterns);
            if deny_clauses.is_empty() {
                lines.push(format!("(allow file-read* (regex #\"{}\"))", regex));
            } else {
                let mut parts = vec![format!("(regex #\"{}\")", regex)];
                parts.extend(deny_clauses);
                let body = parts.join("\n    ");
                lines.push(format!(
                    "(allow file-read*\n  (require-all\n    {}\n  )\n)",
                    body
                ));
            }
        }
    }

    {
        let wide_roots: Vec<&String> = readonly_roots.iter().filter(|r| r.ends_with('/')).collect();
        if !wide_roots.is_empty() {
            lines.push("; readonly（deny write）".into());
            for root in &wide_roots {
                lines.push(format!("(deny file-write* (subpath \"{}\"))", root));
            }
            lines.push("".into());
        }
    }

    if !writable_roots.is_empty() {
        lines.push("; writable roots".into());
        for root in &writable_roots {
            let root_clause = if root.ends_with('/') {
                format!("(subpath \"{}\")", root)
            } else {
                let escaped = regex_escape(root);
                format!("(regex #\"^{}/.*\")", escaped)
            };
            let deny_clauses = build_deny_clauses_for_root(root, &policy.deny_patterns);

            if deny_clauses.is_empty() {
                lines.push(format!("(allow file-read* file-write* {})", root_clause));
            } else {
                let mut parts = vec![root_clause];
                parts.extend(deny_clauses);
                let body = parts.join("\n    ");
                lines.push(format!(
                    "(allow file-read* file-write*\n  (require-all\n    {}\n  )\n)",
                    body
                ));
            }
        }
    }

    if !policy.writable_patterns.is_empty() {
        lines.push("; writable patterns（regex）".into());
        let deny_clauses = build_deny_regex_clauses_all(&policy.deny_patterns);

        for pattern in &policy.writable_patterns {
            let regex = glob_to_seatbelt_regex(pattern);

            if deny_clauses.is_empty() {
                lines.push(format!(
                    "(allow file-read* file-write* (regex #\"{}\"))",
                    regex
                ));
            } else {
                let mut parts = vec![format!("(regex #\"{}\")", regex)];
                parts.extend(deny_clauses.clone());
                let body = parts.join("\n    ");
                lines.push(format!(
                    "(allow file-read* file-write*\n  (require-all\n    {}\n  )\n)",
                    body
                ));
            }
        }
    }

    lines.push("".into());

    {
        let exact_roots: Vec<&String> = readonly_roots
            .iter()
            .filter(|r| !r.ends_with('/'))
            .collect();
        let has_exact = !exact_roots.is_empty() || !policy.readonly_patterns.is_empty();
        if has_exact {
            lines.push("; readonly（deny write，精确）".into());
            for root in &exact_roots {
                lines.push(format!("(deny file-write* (literal \"{}\"))", root));
            }
            for pattern in &policy.readonly_patterns {
                let regex = glob_to_seatbelt_regex(pattern);
                lines.push(format!("(deny file-write* (regex #\"{}\"))", regex));
            }
            lines.push("".into());
        }
    }

    let exact_readonly: Vec<&String> = readonly_roots
        .iter()
        .filter(|r| !r.ends_with('/'))
        .collect();
    if !exact_readonly.is_empty() {
        lines.push("; readonly（allow read，精确路径）".into());
        for root in &exact_readonly {
            let escaped = regex_escape(root);
            lines.push(format!("(allow file-read* (regex #\"^{}$\"))", escaped));
        }
        lines.push("".into());
    }

    lines.join("\n")
}

fn build_deny_clauses_for_root(root: &str, deny_patterns: &[String]) -> Vec<String> {
    let mut clauses = Vec::new();
    let root_prefix = if root.ends_with('/') {
        root.to_string()
    } else {
        format!("{}/", root)
    };

    for pattern in deny_patterns {
        if pattern.starts_with('/') || pattern.starts_with('~') {
            let resolved = resolve_abs(pattern);
            if resolved.starts_with(&root_prefix) || resolved == root.trim_end_matches('/') {
                clauses.push(format!("(require-not (literal \"{}\"))", resolved));
                clauses.push(format!("(require-not (subpath \"{}\"))", resolved));
            }
        } else {
            let regex = glob_to_seatbelt_regex(pattern);

            let suffix = regex.strip_prefix("(^|/)").unwrap_or(&regex);
            clauses.push(format!("(require-not (regex #\"{}\"))", suffix));
        }
    }

    clauses
}

fn build_deny_regex_clauses_all(deny_patterns: &[String]) -> Vec<String> {
    let mut clauses = Vec::new();

    for pattern in deny_patterns {
        if pattern.starts_with('/') || pattern.starts_with('~') {
            let resolved = resolve_abs(pattern);
            clauses.push(format!("(require-not (literal \"{}\"))", resolved));
            clauses.push(format!("(require-not (subpath \"{}\"))", resolved));
        } else {
            let regex = glob_to_seatbelt_regex(pattern);
            clauses.push(format!("(require-not (regex #\"{}\"))", regex));
        }
    }

    clauses
}

fn resolve_abs(pattern: &str) -> String {
    if pattern.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        return pattern.replacen('~', &home, 1);
    }
    pattern.to_string()
}

fn glob_to_seatbelt_regex(pattern: &str) -> String {
    if pattern == "*" {
        return ".*".into();
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return format!("{}$", regex_escape(suffix));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return format!("{}[^/]*$", regex_escape(prefix));
    }
    if let Some(rest) = pattern.strip_suffix('/') {
        return format!("(^|/){}/?", regex_escape(rest));
    }
    format!("(^|/){}$", regex_escape(pattern))
}

fn regex_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        match ch {
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' | '"' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

/// macOS 上 `/var`、`/tmp`、`/etc` 是 `/private/var`、`/private/tmp`、`/private/etc` 的 symlink。
/// seatbelt 的 subpath/regex 规则不跟随 symlink，需要为这些路径生成双重规则。
fn expand_symlink_roots(roots: &[String]) -> Vec<String> {
    let mut expanded: Vec<String> = Vec::with_capacity(roots.len() * 2);
    for root in roots {
        expanded.push(root.clone());
        if let Some(rest) = root.strip_prefix("/var/") {
            expanded.push(format!("/private/var/{}", rest));
        } else if let Some(rest) = root.strip_prefix("/tmp/") {
            expanded.push(format!("/private/tmp/{}", rest));
        } else if let Some(rest) = root.strip_prefix("/etc/") {
            expanded.push(format!("/private/etc/{}", rest));
        } else if root == "/var" || root == "/var/" {
            expanded.push("/private/var".to_string());
        } else if root == "/tmp" || root == "/tmp/" {
            expanded.push("/private/tmp".to_string());
        } else if root == "/etc" || root == "/etc/" {
            expanded.push("/private/etc".to_string());
        }
    }
    expanded
}

#[cfg(test)]
#[path = "seatbelt_test.rs"]
mod tests;
