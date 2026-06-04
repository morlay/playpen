use crate::policy::ShellPolicy;

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedShell {
    pub commands: Vec<Vec<String>>,
    pub has_pipe: bool,
    pub has_multiple: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShellCheckResult {
    pub allowed: bool,
    pub matched_pattern: Option<String>,
    pub reason: Option<String>,
}

pub struct ShellConfig {
    pub allow_pipe: bool,
    pub allow_multiple: bool,
}

/// 将命令行参数列表安全拼接为 shell 命令字符串。
/// 自动为包含特殊字符（如 `&`、`|`、空格等）的参数添加必要的引号保护，
/// 避免被 shell 解释为控制操作符。
pub fn join_args(args: &[String]) -> Result<String, String> {
    shlex::try_join(args.iter().map(|s| s.as_str()))
        .map_err(|e| format!("命令参数拼接失败: {e}"))
}

/// 用 shlex 解析命令字符串，检测管道和命令串联。
pub fn parse_shell(command: &str) -> ParsedShell {
    let mut commands: Vec<Vec<String>> = Vec::new();
    let mut has_pipe = false;
    let mut has_multiple = false;

    // 先扫描找到分隔符位置，切分后逐段用 shlex 解析
    let mut start = 0;
    let bytes = command.as_bytes();

    for (i, &b) in bytes.iter().enumerate() {
        let delim = if b == b'|' && bytes.get(i + 1) != Some(&b'|') {
            has_pipe = true;
            Some(1)
        } else if b == b'&' && bytes.get(i + 1) == Some(&b'&') {
            has_multiple = true;
            Some(2)
        } else if b == b';' {
            has_multiple = true;
            Some(1)
        } else {
            None
        };

        if let Some(len) = delim {
            let segment = &command[start..i];
            if let Some(words) = shlex::split(segment)
                && !words.is_empty()
            {
                commands.push(words);
            }
            start = i + len;
        }
    }

    let segment = &command[start..];
    if let Some(words) = shlex::split(segment)
        && !words.is_empty()
    {
        commands.push(words);
    }

    ParsedShell {
        commands,
        has_pipe,
        has_multiple,
    }
}

pub fn check_shell(
    command: &str,
    config: &ShellConfig,
    exec_policy: &ShellPolicy,
) -> ShellCheckResult {
    let parsed = parse_shell(command);

    if !config.allow_multiple && parsed.has_multiple {
        return ShellCheckResult {
            allowed: false,
            matched_pattern: None,
            reason: Some("不允许使用多命令串联（&&、;）".to_string()),
        };
    }

    if !config.allow_pipe && parsed.has_pipe {
        return ShellCheckResult {
            allowed: false,
            matched_pattern: None,
            reason: Some("不允许使用管道（|）".to_string()),
        };
    }

    for cmd in &parsed.commands {
        if cmd.is_empty() {
            continue;
        }

        match exec_policy.check(cmd) {
            Some(true) => {}
            Some(false) => {
                return ShellCheckResult {
                    allowed: false,
                    matched_pattern: Some(cmd.join(" ")),
                    reason: Some(format!("禁止执行: {}", cmd.join(" "))),
                };
            }
            None => {
                return ShellCheckResult {
                    allowed: false,
                    matched_pattern: None,
                    reason: Some(format!("{}：未匹配任何允许规则", cmd.join(" "))),
                };
            }
        }
    }

    ShellCheckResult {
        allowed: true,
        matched_pattern: None,
        reason: None,
    }
}

#[cfg(test)]
#[path = "shell_test.rs"]
mod tests;
