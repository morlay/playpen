use std::path::Path;

use super::{ParsedRule, RulePrefix, ValidationResult};

pub fn parse_filesystem_access(access: &[String]) -> Vec<ParsedRule> {
    let lines: Vec<String> = access
        .iter()
        .flat_map(|s| s.lines().map(|l| l.to_string()))
        .collect();
    parse_filesystem_rules(&lines)
}

pub fn parse_network_access(access: &[String]) -> Vec<ParsedRule> {
    let lines: Vec<String> = access
        .iter()
        .flat_map(|s| s.lines().map(|l| l.to_string()))
        .collect();
    parse_network_rules(&lines)
}

pub fn parse_filesystem_rules(rules: &[String]) -> Vec<ParsedRule> {
    parse_rules(rules, |trimmed| {
        if let Some(rest) = trimmed.strip_prefix("rw") {
            (RulePrefix::Allow, rest.trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("r-") {
            (RulePrefix::ReadOnly, rest.trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("--") {
            (RulePrefix::Deny, rest.trim().to_string())
        } else {
            (RulePrefix::Allow, trimmed.to_string())
        }
    })
}

pub fn parse_network_rules(rules: &[String]) -> Vec<ParsedRule> {
    parse_rules(rules, |trimmed| {
        if let Some(rest) = trimmed.strip_prefix('!') {
            (RulePrefix::Deny, rest.to_string())
        } else {
            (RulePrefix::Allow, trimmed.to_string())
        }
    })
}

fn parse_rules(
    rules: &[String],
    classify: impl Fn(&str) -> (RulePrefix, String),
) -> Vec<ParsedRule> {
    let mut out = Vec::new();
    for raw in rules {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (prefix, pattern) = classify(trimmed);
        out.push(ParsedRule {
            raw: trimmed.to_string(),
            prefix,
            pattern,
        });
    }
    out
}

pub fn parse_filesystem_string(raw: &str) -> Vec<ParsedRule> {
    let lines: Vec<String> = raw.lines().map(|s| s.to_string()).collect();
    parse_filesystem_rules(&lines)
}

pub fn parse_network_string(raw: &str) -> Vec<ParsedRule> {
    let lines: Vec<String> = raw.lines().map(|s| s.to_string()).collect();
    parse_network_rules(&lines)
}

pub fn validate_filesystem_path(
    rules: &[ParsedRule],
    cwd: &Path,
    target: &Path,
) -> ValidationResult {
    if rules.is_empty() {
        return ValidationResult::Denied;
    }

    for rule in rules {
        if filesystem_pattern_matches(&rule.pattern, target, cwd) {
            return match rule.prefix {
                RulePrefix::Allow => ValidationResult::Allowed,
                RulePrefix::Deny => ValidationResult::Denied,
                RulePrefix::ReadOnly => ValidationResult::ReadOnly,
            };
        }
    }

    ValidationResult::Denied
}

pub fn find_filesystem_rule<'a>(
    rules: &'a [ParsedRule],
    cwd: &Path,
    target: &Path,
) -> Option<&'a ParsedRule> {
    rules
        .iter()
        .find(|r| filesystem_pattern_matches(&r.pattern, target, cwd))
}

pub fn validate_network_domain(rules: &[ParsedRule], domain: &str) -> ValidationResult {
    if rules.is_empty() {
        return ValidationResult::Denied;
    }

    for rule in rules {
        if simple_glob_match(&rule.pattern, domain) {
            return match rule.prefix {
                RulePrefix::Allow => ValidationResult::Allowed,
                RulePrefix::Deny => ValidationResult::Denied,
                RulePrefix::ReadOnly => ValidationResult::Denied,
            };
        }
    }

    ValidationResult::Denied
}

pub fn is_path_pattern(pattern: &str) -> bool {
    pattern.contains('/') || pattern.starts_with('~') || pattern == "."
}

fn filesystem_pattern_matches(pattern: &str, target: &Path, cwd: &Path) -> bool {
    if is_path_pattern(pattern) {
        let resolved = resolve_pattern(pattern, cwd);
        return target.to_string_lossy().starts_with(&resolved);
    }

    if let Some(filename) = target.file_name().and_then(|f| f.to_str()) {
        return simple_glob_match(pattern, filename);
    }

    false
}

pub fn simple_glob_match(pattern: &str, target: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == target;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0usize;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !target.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            return target[pos..].ends_with(part);
        } else if let Some(found) = target[pos..].find(part) {
            pos += found + part.len();
        } else {
            return false;
        }
    }

    true
}

pub fn resolve_pattern(pattern: &str, cwd: &Path) -> String {
    if pattern == "." {
        return cwd.to_string_lossy().to_string();
    }

    if pattern.contains('/') || pattern.starts_with('~') {
        if pattern.starts_with('~') {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            return pattern.replacen('~', &home, 1);
        }
        if pattern.starts_with('/') {
            return pattern.to_string();
        }
        return cwd.join(pattern).to_string_lossy().to_string();
    }

    pattern.to_string()
}

#[cfg(test)]
#[path = "parser_test.rs"]
mod tests;
