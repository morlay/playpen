use crate::policy::PolicyClassification;

const BASE_POLICY: &str = include_str!("seatbelt_base_policy.sbpl");
const NETWORK_POLICY: &str = include_str!("seatbelt_network_policy.sbpl");
const PLATFORM_DEFAULTS: &str =
    include_str!("seatbelt_restricted_read_only_platform_defaults.sbpl");

/// 基于 openai/codex 策略生成 sandbox-exec profile。
///
/// 使用 `(require-all (require-not ...))` 将 deny 模式嵌入到 writable allow 规则内部，
/// 避免独立 deny 规则与 allow 规则之间的优先级问题。
pub fn generate_profile(policy: &PolicyClassification) -> String {
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

    let has_rules = !policy.writable_roots.is_empty()
        || !policy.writable_patterns.is_empty()
        || !policy.readonly_roots.is_empty()
        || !policy.readonly_patterns.is_empty();

    if !has_rules {
        return lines.join("\n");
    }

    let has_readonly = !policy.readonly_roots.is_empty() || !policy.readonly_patterns.is_empty();

    // === readonly allow read（宽泛路径，嵌入 deny require-not） ===
    // 以 / 结尾的 root（目录树）和 patterns 在此输出；精确路径在后面对输出
    if has_readonly {
        lines.push("; readonly（allow read）".into());
        for root in &policy.readonly_roots {
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

    // === readonly deny write（宽泛 roots，放在 writable allow 之前让 allow 覆盖） ===
    // 仅以 / 结尾的 readonly roots（大目录树）在此输出，patterns 放到后面
    {
        let wide_roots: Vec<&String> = policy
            .readonly_roots
            .iter()
            .filter(|r| r.ends_with('/'))
            .collect();
        if !wide_roots.is_empty() {
            lines.push("; readonly（deny write）".into());
            for root in &wide_roots {
                lines.push(format!("(deny file-write* (subpath \"{}\"))", root));
            }
            lines.push("".into());
        }
    }

    // === writable roots（嵌入 deny require-not） ===
    if !policy.writable_roots.is_empty() {
        lines.push("; writable roots".into());
        for root in &policy.writable_roots {
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

    // === writable patterns（嵌入所有 deny require-not） ===
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

    // === readonly deny write（精确路径 + patterns，放在 writable allow 之后覆盖 allow） ===
    // 不以 / 结尾的 readonly roots + 所有 readonly patterns
    {
        let exact_roots: Vec<&String> = policy
            .readonly_roots
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

    // === readonly allow read（精确路径，不嵌入 deny，放在最后覆盖 writable require-not） ===
    // 仅对不以 / 结尾的 readonly roots（即具体文件路径）单独输出
    let exact_readonly: Vec<&String> = policy
        .readonly_roots
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

/// 为 writable root 构建 deny require-not 子句。
///
/// - 绝对路径 deny 且在 root 下的：生成 `(require-not (literal ...))` + `(require-not (subpath ...))`
/// - 相对模式 deny：生成 `(require-not (regex ...))`，去掉 `(^|/)` 前缀
///   （因为 require-all 中已有 subpath/regex 约束路径前缀）
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
            // 去掉 (^|/) 前缀，因为外层 require-all 已有路径前缀约束
            let suffix = regex.strip_prefix("(^|/)").unwrap_or(&regex);
            clauses.push(format!("(require-not (regex #\"{}\"))", suffix));
        }
    }

    clauses
}

/// 为 writable_pattern 构建 deny require-not 子句（所有 deny 都嵌入）。
///
/// 与 writable root 不同，pattern 使用 regex 匹配任意位置，
/// 因此绝对路径 deny 也需要嵌入。
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

#[cfg(test)]
#[path = "seatbelt_test.rs"]
mod tests;
