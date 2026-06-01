use crate::config::{ParsedRule, RulePrefix};

/// Shell 命令权限规则
#[derive(Debug, Clone)]
pub struct ShellRule {
    pub raw: String,
    pub command_name: String,
    pub arg_patterns: Vec<String>,
    pub allowed: bool,
}

/// Shell 权限策略
#[derive(Debug, Clone, Default)]
pub struct ShellPolicy {
    rules: Vec<ShellRule>,
}

impl ShellPolicy {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn from_raw(raw: &str) -> Self {
        let mut rules = Vec::new();

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let (pattern, allowed) = if let Some(rest) = trimmed.strip_prefix('!') {
                (rest.trim(), false)
            } else {
                (trimmed, true)
            };

            let parts: Vec<String> = pattern
                .split_whitespace()
                .filter(|p| *p != "*")
                .map(|s| s.to_string())
                .collect();

            if parts.is_empty() {
                continue;
            }

            rules.push(ShellRule {
                raw: trimmed.to_string(),
                command_name: parts[0].clone(),
                arg_patterns: parts[1..].to_vec(),
                allowed,
            });
        }

        rules.sort_by(|a, b| {
            b.arg_patterns
                .len()
                .cmp(&a.arg_patterns.len())
                .then_with(|| a.raw.len().cmp(&b.raw.len()))
        });

        ShellPolicy { rules }
    }

    pub fn check(&self, cmd: &[String]) -> Option<bool> {
        if self.rules.is_empty() {
            return Some(true);
        }

        if cmd.is_empty() {
            return Some(false);
        }

        let non_flag_args: Vec<&String> = cmd[1..].iter().filter(|a| !a.starts_with('-')).collect();

        for rule in &self.rules {
            if rule.command_name != "*" && rule.command_name != cmd[0] {
                continue;
            }

            if non_flag_args.len() < rule.arg_patterns.len() {
                continue;
            }

            let prefix_match = rule
                .arg_patterns
                .iter()
                .zip(non_flag_args.iter())
                .all(|(pat, arg)| *pat == **arg);

            if prefix_match {
                return Some(rule.allowed);
            }
        }

        None
    }
}

/// 从解析后的规则列表构建文件系统策略分类。
///
/// 路径分类：
/// - `/abs`、`~/abs` → 绝对路径，subpath 匹配
/// - `./rel` → cwd 相对路径，subpath 匹配
/// - `dir/`（含 `/` 但非绝对/相对）→ 目录模式，regex 匹配（任意位置）
/// - 无 `/` → 文件名模式，regex 匹配
pub fn classify_policy(rules: &[ParsedRule], cwd: &std::path::Path) -> PolicyClassification {
    let mut writable_roots: Vec<String> = Vec::new();
    let mut writable_patterns: Vec<String> = Vec::new();
    let mut readonly_roots: Vec<String> = Vec::new();
    let mut readonly_patterns: Vec<String> = Vec::new();
    let mut deny_patterns: Vec<String> = Vec::new();

    for rule in rules {
        let is_subpath = rule.pattern.starts_with('/')
            || rule.pattern.starts_with('~')
            || rule.pattern.starts_with("./")
            || rule.pattern == ".";
        let is_dir_pattern = !is_subpath && rule.pattern.contains('/');

        match &rule.prefix {
            RulePrefix::Allow if is_subpath => {
                writable_roots.push(resolve_subpath(&rule.pattern, cwd));
            }
            RulePrefix::Allow if is_dir_pattern => {
                writable_patterns.push(rule.pattern.clone());
            }
            RulePrefix::Deny if is_subpath => {
                deny_patterns.push(resolve_subpath(&rule.pattern, cwd));
            }
            RulePrefix::Deny => {
                deny_patterns.push(rule.pattern.clone());
            }
            RulePrefix::ReadOnly if is_subpath => {
                readonly_roots.push(resolve_subpath(&rule.pattern, cwd));
            }
            RulePrefix::ReadOnly => {
                readonly_patterns.push(rule.pattern.clone());
            }
            _ => {}
        }
    }

    PolicyClassification {
        writable_roots,
        writable_patterns,
        deny_patterns,
        readonly_roots,
        readonly_patterns,
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;

fn resolve_subpath(pattern: &str, cwd: &std::path::Path) -> String {
    if pattern == "." || pattern.starts_with("./") {
        let rel = pattern.trim_start_matches('.').trim_start_matches('/');
        if rel.is_empty() {
            return cwd.to_string_lossy().to_string();
        }
        return cwd.join(rel).to_string_lossy().to_string();
    }
    if pattern.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        return pattern.replacen('~', &home, 1);
    }
    pattern.to_string()
}

pub struct PolicyClassification {
    /// Allow 规则的路径（已解析为绝对路径）
    pub writable_roots: Vec<String>,
    /// Allow 规则的文件名模式（目录模式）
    pub writable_patterns: Vec<String>,
    /// Deny 规则的模式（路径或文件名 glob）
    pub deny_patterns: Vec<String>,
    /// ReadOnly 规则的路径（已解析为绝对路径）
    pub readonly_roots: Vec<String>,
    /// ReadOnly 规则的文件名模式（仅禁止写入，不禁止读取）
    pub readonly_patterns: Vec<String>,
}
