#[derive(Debug, Clone)]
pub struct ShellRule {
    pub raw: String,
    pub command_name: String,
    pub arg_patterns: Vec<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ShellPolicy {
    rules: Vec<ShellRule>,
}

impl ShellPolicy {
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn from_rules(rules: &[String]) -> Self {
        Self::from_iter(rules.iter().flat_map(|s| s.lines()))
    }

    #[cfg(test)]
    pub fn from_raw(raw: &str) -> Self {
        Self::from_iter(raw.lines())
    }

    fn from_iter<'a>(lines: impl Iterator<Item = &'a str>) -> Self {
        let mut rules = Vec::new();
        for line in lines {
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
                if pattern == "*" || pattern.split_whitespace().all(|p| p == "*") {
                    rules.push(ShellRule {
                        raw: trimmed.to_string(),
                        command_name: "*".to_string(),
                        arg_patterns: vec![],
                        allowed,
                    });
                }
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
            let a_wildcard = a.command_name == "*";
            let b_wildcard = b.command_name == "*";
            a_wildcard
                .cmp(&b_wildcard)
                .then_with(|| b.arg_patterns.len().cmp(&a.arg_patterns.len()))
                .then_with(|| a.raw.len().cmp(&b.raw.len()))
        });
        ShellPolicy { rules }
    }

    /// 返回所有允许的规则原始文本（用于错误提示）。
    pub fn allowed_patterns(&self) -> Vec<&str> {
        self.rules
            .iter()
            .filter(|r| r.allowed)
            .map(|r| r.raw.as_str())
            .collect()
    }

    pub fn check(&self, cmd: &[String]) -> Option<(&ShellRule, bool)> {
        if self.rules.is_empty() || cmd.is_empty() {
            return None;
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
                return Some((rule, rule.allowed));
            }
        }
        None
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod tests;

#[derive(Debug, Clone, Default)]
pub struct PolicyClassification {
    pub writable_roots: Vec<String>,
    pub writable_patterns: Vec<String>,
    pub deny_patterns: Vec<String>,
    pub readonly_roots: Vec<String>,
    pub readonly_patterns: Vec<String>,
}

impl PolicyClassification {
    /// 从已解析的文件系统规则分类生成 PolicyClassification。
    ///
    /// - 路径类规则（含 `/`、`~` 或 `.`）会被解析为绝对路径，按前缀分类为 root。
    /// - 模式类规则按前缀分类为 pattern。
    /// - `--`（Deny）规则的 pattern 原样放入 `deny_patterns`。
    pub fn from_parsed_rules(rules: &[super::ParsedRule], cwd: &std::path::Path) -> Self {
        let mut this = Self::default();
        for rule in rules {
            if super::is_path_pattern(&rule.pattern) {
                let resolved = super::parser::resolve_pattern(&rule.pattern, cwd);
                match rule.prefix {
                    super::RulePrefix::Allow => this.writable_roots.push(resolved),
                    super::RulePrefix::ReadOnly => this.readonly_roots.push(resolved),
                    super::RulePrefix::Deny => this.deny_patterns.push(rule.pattern.clone()),
                }
            } else {
                match rule.prefix {
                    super::RulePrefix::Allow => this.writable_patterns.push(rule.pattern.clone()),
                    super::RulePrefix::ReadOnly => {
                        this.readonly_patterns.push(rule.pattern.clone())
                    }
                    super::RulePrefix::Deny => this.deny_patterns.push(rule.pattern.clone()),
                }
            }
        }
        this
    }
}
