use serde::Deserialize;
use std::path::Path;

/// config.toml 顶层结构
#[derive(Debug, Deserialize, Default, Clone)]
pub struct Config {
    pub network: Option<AllowSection>,
    pub filesystem: Option<AllowSection>,
    pub shell: Option<ShellSection>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AllowSection {
    pub access: Option<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ShellSection {
    pub allow_pipe: Option<bool>,
    pub allow_multiple: Option<bool>,
    pub allow: Option<String>,
}

/// 单条规则的前缀类型
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RulePrefix {
    /// 允许
    Allow,
    /// 拒绝
    Deny,
    /// 只读
    ReadOnly,
}

/// 解析后的一条规则
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ParsedRule {
    pub raw: String,
    pub prefix: RulePrefix,
    pub pattern: String,
}

/// 解析 access/allow 字符串，按行拆分并识别前缀。
/// 空行和 # 开头的注释行会被跳过。
///
/// 前缀（filesystem）：
/// - `rw ` → 读写允许 (Allow)
/// - `r- ` → 只读 (ReadOnly)
/// - `-- ` → 拒绝 (Deny)
/// - 无前缀 → 允许 (Allow)
pub fn parse_filesystem_string(raw: &str) -> Vec<ParsedRule> {
    let mut rules = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (prefix, pattern) = if let Some(rest) = trimmed.strip_prefix("rw") {
            (RulePrefix::Allow, rest.trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("r-") {
            (RulePrefix::ReadOnly, rest.trim().to_string())
        } else if let Some(rest) = trimmed.strip_prefix("--") {
            (RulePrefix::Deny, rest.trim().to_string())
        } else {
            (RulePrefix::Allow, trimmed.to_string())
        };

        rules.push(ParsedRule {
            raw: trimmed.to_string(),
            prefix,
            pattern,
        });
    }

    rules
}

/// 解析 network access 字符串，按行拆分并识别前缀。
/// 空行和 # 开头的注释行会被跳过。
///
/// 前缀（network）：
/// - `!`  → 拒绝 (Deny)
/// - 无前缀 → 允许 (Allow)
pub fn parse_network_string(raw: &str) -> Vec<ParsedRule> {
    let mut rules = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (prefix, pattern) = if let Some(rest) = trimmed.strip_prefix('!') {
            (RulePrefix::Deny, rest.to_string())
        } else {
            (RulePrefix::Allow, trimmed.to_string())
        };

        rules.push(ParsedRule {
            raw: trimmed.to_string(),
            prefix,
            pattern,
        });
    }

    rules
}

/// 验证结果
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValidationResult {
    /// 允许
    Allowed,
    /// 拒绝
    Denied,
    /// 只读
    ReadOnly,
}

/// 验证给定路径是否匹配 filesystem 规则。
/// 沙箱默认拒绝——无规则或有规则但未命中时均返回 Denied。
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

/// 返回匹配给定路径的规则（用于打印规则详情）
pub fn find_filesystem_rule<'a>(
    rules: &'a [ParsedRule],
    cwd: &Path,
    target: &Path,
) -> Option<&'a ParsedRule> {
    rules
        .iter()
        .find(|r| filesystem_pattern_matches(&r.pattern, target, cwd))
}

/// 验证给定域名是否匹配 network 规则。
/// 沙箱默认拒绝——无规则或有规则但未命中时均返回 Denied。
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

/// 检查 target 路径是否匹配 filesystem 规则模式
fn filesystem_pattern_matches(pattern: &str, target: &Path, cwd: &Path) -> bool {
    // 路径模式（包含 /、~、或为 .）
    if pattern.contains('/') || pattern.starts_with('~') || pattern == "." {
        let resolved = resolve_pattern(pattern, cwd);
        return target.to_string_lossy().starts_with(&resolved);
    }

    // 文件名模式：匹配 target 的文件名组件
    if let Some(filename) = target.file_name().and_then(|f| f.to_str()) {
        return simple_glob_match(pattern, filename);
    }

    false
}

/// 简单的 glob 匹配，仅支持 `*` 通配符
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

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;

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
