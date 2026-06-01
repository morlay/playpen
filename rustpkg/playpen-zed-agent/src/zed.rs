use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Serialize;

use sandbox::config::{self, Config, ParsedRule, RulePrefix};

/// 工具名 → 权限映射
pub type ToolPermissions = HashMap<String, ZedToolPermission>;

/// 生成 zed agent 配置。write 为 true 时写入 settings.json（自动备份），否则输出到 stdout。
pub fn setup_zed_agent(
    sandbox_config: &Config,
    cwd: &Path,
    profile_name: &str,
    global_settings: &Path,
    project_settings: Option<&Path>,
    write: bool,
) -> anyhow::Result<()> {
    let filesystem_rules = sandbox_config
        .filesystem
        .as_ref()
        .and_then(|f| f.access.as_deref())
        .map(config::parse_filesystem_string)
        .unwrap_or_default();

    let tool_permissions = generate_tool_permissions(&filesystem_rules, cwd);
    let profiles = generate_profiles(profile_name);

    let tool_permissions = filter_by_profile(&tool_permissions, &profiles, profile_name);

    apply_settings(global_settings, &tool_permissions, &profiles, write)?;

    if let Some(project_path) = project_settings {
        if write && let Some(parent) = project_path.parent() {
            fs::create_dir_all(parent)?;
        }
        apply_settings(project_path, &tool_permissions, &profiles, write)?;
    }

    Ok(())
}

// ---- 数据结构 ----

#[derive(Serialize, Clone)]
pub struct ZedToolPermission {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_allow: Option<Vec<ZedPattern>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_deny: Option<Vec<ZedPattern>>,
}

#[derive(Serialize, Clone)]
pub struct ZedPattern {
    pub pattern: String,
}

// ---- 生成 tool_permissions ----

pub fn generate_tool_permissions(rules: &[ParsedRule], cwd: &Path) -> ToolPermissions {
    let mut allow_patterns: Vec<String> = Vec::new();
    let mut deny_patterns: Vec<String> = Vec::new();
    let readonly_patterns: Vec<String> = Vec::new();

    for rule in rules {
        if rule.pattern == "." {
            continue;
        }
        // r- 规则不映射到 zed（zed 的 always_deny 太粗暴，语义不匹配），
        // 由 seatbelt 沙箱兜底确保只读
        if rule.prefix == RulePrefix::ReadOnly {
            continue;
        }
        let pattern = pattern_to_zed_regex(&rule.pattern, cwd);
        match rule.prefix {
            RulePrefix::Allow => dedup_push(&mut allow_patterns, pattern),
            RulePrefix::Deny => dedup_push(&mut deny_patterns, pattern),
            RulePrefix::ReadOnly => {} // unreachable
        }
    }

    let to_patterns = |v: &[String]| -> Vec<ZedPattern> {
        v.iter()
            .map(|p| ZedPattern { pattern: p.clone() })
            .collect()
    };

    let allow = to_patterns(&allow_patterns);
    let deny = to_patterns(&deny_patterns);
    let readonly = to_patterns(&readonly_patterns);

    let mut read_allow = allow.clone();
    read_allow.extend(readonly.clone());

    let mut write_deny = deny.clone();
    write_deny.extend(readonly.clone());

    let non_empty = |v: Vec<ZedPattern>| -> Option<Vec<ZedPattern>> {
        if v.is_empty() { None } else { Some(v) }
    };

    let mut perms = HashMap::new();

    for tool in crate::tools::TOOLS {
        if !tool.enabled {
            continue;
        }
        let perm = match tool.category {
            crate::tools::ToolCategory::FilesystemRead => ZedToolPermission {
                default: tool.default_permission.map(|s| s.to_string()),
                always_allow: non_empty(read_allow.clone()),
                always_deny: non_empty(deny.clone()),
            },
            crate::tools::ToolCategory::FilesystemWrite => ZedToolPermission {
                default: tool.default_permission.map(|s| s.to_string()),
                always_allow: non_empty(allow.clone()),
                always_deny: non_empty(write_deny.clone()),
            },
            crate::tools::ToolCategory::Shell => ZedToolPermission {
                default: tool.default_permission.map(|s| s.to_string()),
                always_allow: Some(vec![ZedPattern {
                    pattern: "^playpen\\b".into(),
                }]),
                always_deny: None,
            },
            crate::tools::ToolCategory::Network => ZedToolPermission {
                default: tool.default_permission.map(|s| s.to_string()),
                always_allow: None,
                always_deny: None,
            },
            crate::tools::ToolCategory::Other => ZedToolPermission {
                default: tool.default_permission.map(|s| s.to_string()),
                always_allow: None,
                always_deny: None,
            },
        };
        perms.insert(tool.name.to_string(), perm);
    }

    perms
}

/// 将 sandbox 模式转换为 zed 兼容的正则
pub fn pattern_to_zed_regex(pattern: &str, _cwd: &Path) -> String {
    if pattern.contains('/') || pattern.starts_with('~') {
        if pattern.starts_with('~') {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            let absolute = pattern.replacen('~', &home, 1);
            return format!("^{}", regex_escape(&absolute));
        }
        if pattern.starts_with('/') {
            return format!("^{}", regex_escape(pattern));
        }
        let escaped = regex_escape(pattern);
        let with_boundary = if pattern.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            format!("\\b{}", escaped)
        } else {
            escaped
        };
        if pattern.ends_with('/') {
            format!("{}?", with_boundary)
        } else {
            with_boundary
        }
    } else {
        if pattern == "*" {
            return ".*".into();
        }
        if let Some(suffix) = pattern.strip_prefix('*') {
            return format!("{}$", regex_escape(suffix));
        }
        if let Some(prefix) = pattern.strip_suffix('*') {
            return format!("{}[^/]*$", regex_escape(prefix));
        }
        format!("{}$", regex_escape(pattern))
    }
}

fn regex_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        match ch {
            '.' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

fn dedup_push(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

// ---- 生成 profiles ----

fn generate_profiles(profile_name: &str) -> serde_json::Value {
    let mut tools = serde_json::Map::new();
    for tool in crate::tools::TOOLS {
        tools.insert(tool.name.to_string(), serde_json::Value::Bool(tool.enabled));
    }
    serde_json::json!({
        profile_name: {
            "tools": tools
        }
    })
}

fn filter_by_profile(
    perms: &ToolPermissions,
    profiles: &serde_json::Value,
    profile_name: &str,
) -> ToolPermissions {
    let tools_on = profiles
        .pointer(&format!("/{}/tools", profile_name))
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    let mut filtered = HashMap::new();
    for (name, perm) in perms {
        if tools_on.get(name).copied().unwrap_or(true) {
            filtered.insert(name.clone(), perm.clone());
        }
    }
    filtered
}

// ---- settings.json 读写 ----

fn apply_settings(
    path: &Path,
    tool_permissions: &ToolPermissions,
    profiles: &serde_json::Value,
    write: bool,
) -> anyhow::Result<()> {
    let (old_raw, old_root) = if path.exists() {
        let raw = fs::read_to_string(path)?;
        let parsed: serde_json::Value = match json5::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "警告: 解析 {} 失败: {}, 将视为空文件继续",
                    path.display(),
                    e
                );
                serde_json::json!({})
            }
        };
        (Some(raw), parsed)
    } else {
        (None, serde_json::json!({}))
    };

    let old_agent = old_root
        .get("agent")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let new_agent = build_agent(&old_agent, tool_permissions, profiles);
    let new_agent_str = serde_json::to_string_pretty(&new_agent)?;

    if !write {
        let old_str = serde_json::to_string_pretty(&old_root).unwrap_or_default();
        println!("// === 原始: {} ===", path.display());
        if let Some(ref r) = old_raw {
            println!("{}", r.trim());
        } else {
            println!("{}", old_str);
        }
        println!("// === 合并后: {} ===", path.display());
        if let Some(ref r) = old_raw {
            println!("{}", replace_agent_in_text(r, &new_agent_str));
        } else {
            let mut new_root = old_root;
            new_root["agent"] = new_agent;
            println!("{}", serde_json::to_string_pretty(&new_root)?);
        }
        return Ok(());
    }

    let output = if let Some(ref r) = old_raw {
        replace_agent_in_text(r, &new_agent_str)
    } else {
        let mut new_root = old_root;
        new_root["agent"] = new_agent;
        serde_json::to_string_pretty(&new_root)?
    };

    if path.exists() {
        let bak = path.with_extension("json.bak");
        fs::copy(path, &bak)?;
    }
    fs::write(path, output + "\n")?;
    eprintln!("已写入: {}", path.display());

    Ok(())
}

fn replace_agent_in_text(text: &str, new_agent_json: &str) -> String {
    let key = "\"agent\"";
    let Some(pos) = text.find(key) else {
        let trimmed = text.trim_end();
        let indent = detect_indent(text);
        return format!(
            "{}\
{}// === playpen 生成 ===\n{}\"agent\": {}\n}}",
            trimmed.trim_end_matches(|c: char| c.is_whitespace()),
            indent,
            indent,
            indent_json(new_agent_json, &indent),
        );
    };

    let after_key = &text[pos + key.len()..];
    let Some(colon_pos) = after_key.find(':') else {
        return text.to_string();
    };
    let value_start = pos + key.len() + colon_pos + 1;

    let rest = &text[value_start..];
    let Some(brace_start) = rest.find(|c: char| !c.is_whitespace()) else {
        return text.to_string();
    };
    if rest.as_bytes()[brace_start] != b'{' {
        return text.to_string();
    };

    let abs_start = value_start + brace_start;

    let mut depth = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut end = abs_start;
    for (i, ch) in text[abs_start..].char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '"' => in_string = !in_string,
            '\\' if in_string => escape = true,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    end = abs_start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    let indent = detect_indent_at(text, pos);

    let mut result = String::with_capacity(text.len());
    result.push_str(&text[..abs_start]);
    result.push_str(&indent_json(new_agent_json, &indent));
    result.push_str(&text[end..]);
    result
}

fn detect_indent(text: &str) -> String {
    for line in text.lines() {
        if let Some(indent) = line.find(|c: char| !c.is_whitespace())
            && indent > 0
        {
            return line[..indent].to_string();
        }
    }
    "  ".to_string()
}

fn detect_indent_at(text: &str, pos: usize) -> String {
    let prefix = &text[..pos];
    for line in prefix.lines().rev() {
        if let Some(indent) = line.find(|c: char| !c.is_whitespace())
            && indent > 0
        {
            return line[..indent].to_string();
        }
    }
    "  ".to_string()
}

fn indent_json(json: &str, indent: &str) -> String {
    json.lines()
        .map(|l| format!("{}{}", indent, l))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn build_agent(
    existing_agent: &serde_json::Value,
    tool_permissions: &ToolPermissions,
    profiles: &serde_json::Value,
) -> serde_json::Value {
    let mut agent = existing_agent.clone();

    let tools_key = "tool_permissions";
    let mut tp = agent
        .get(tools_key)
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let mut tools = tp.get("tools").cloned().unwrap_or(serde_json::json!({}));

    for (name, perm) in tool_permissions {
        merge_tool(&mut tools, name, perm);
    }

    tp["tools"] = tools;
    agent[tools_key] = tp;

    if let Some(new_profiles) = profiles.as_object() {
        let mut existing_profiles = agent
            .get("profiles")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        for (profile_name, new_profile) in new_profiles {
            let mut p = existing_profiles
                .get(profile_name)
                .cloned()
                .unwrap_or(serde_json::json!({}));
            if let Some(new_tools) = new_profile.get("tools") {
                p["tools"] = new_tools.clone();
            }
            existing_profiles.insert(profile_name.clone(), p);
        }
        agent["profiles"] = serde_json::Value::Object(existing_profiles);
    }

    agent
}

fn merge_tool(tools: &mut serde_json::Value, name: &str, perm: &ZedToolPermission) {
    if perm.always_allow.is_none() && perm.always_deny.is_none() && perm.default.is_none() {
        return;
    }
    let mut tool = if let Some(existing) = tools.get(name).cloned() {
        existing
    } else {
        serde_json::json!({})
    };

    if let Some(ref default_val) = perm.default
        && tool.get("default").is_none()
    {
        tool["default"] = serde_json::Value::String(default_val.clone());
    }

    if let Some(ref allow) = perm.always_allow {
        tool["always_allow"] = serde_json::to_value(allow).unwrap_or_default();
    }
    if let Some(ref deny) = perm.always_deny {
        tool["always_deny"] = serde_json::to_value(deny).unwrap_or_default();
    }

    tools[name] = tool;
}

#[cfg(test)]
#[path = "zed_test.rs"]
mod tests;
