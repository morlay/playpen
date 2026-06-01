use flash::lexer::Lexer;
use flash::parser::{Node, Parser};

use super::policy::ShellPolicy;

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

/// 将命令参数格式化为 `` `name ...` `` 风格，用于统一错误提示。
pub fn display_cmd(cmd: &[String]) -> String {
    if cmd.is_empty() {
        return String::new();
    }
    format!("`{} ...`", cmd[0])
}

pub fn join_args(args: &[String]) -> Result<String, String> {
    shlex::try_join(args.iter().map(|s| s.as_str())).map_err(|e| format!("命令参数拼接失败: {e}"))
}

fn collect_commands(node: &Node) -> (Vec<Vec<String>>, bool, bool) {
    let mut commands = Vec::new();
    let mut has_pipe = false;
    let mut has_multiple = false;
    walk_node(node, &mut commands, &mut has_pipe, &mut has_multiple);
    (commands, has_pipe, has_multiple)
}

fn walk_node(
    node: &Node,
    commands: &mut Vec<Vec<String>>,
    has_pipe: &mut bool,
    has_multiple: &mut bool,
) {
    match node {
        Node::List {
            statements,
            operators: _,
        } => {
            // 只有列表中有多个 statement 才算 has_multiple
            if statements.len() > 1 {
                *has_multiple = true;
            }
            for stmt in statements {
                walk_node(stmt, commands, has_pipe, has_multiple);
            }
        }
        Node::Pipeline {
            commands: pipeline_commands,
        } => {
            *has_pipe = true;
            for cmd in pipeline_commands {
                walk_node(cmd, commands, has_pipe, has_multiple);
            }
        }
        Node::Command { name, args, .. } | Node::FunctionCall { name, args, .. } => {
            let mut words = vec![name.clone()];
            words.extend(args.iter().cloned());
            commands.push(words);
        }
        Node::Negation { command } => {
            walk_node(command, commands, has_pipe, has_multiple);
        }
        Node::Assignment { name: _, value: _ } => {
            // VAR=value cmd 形式：赋值本身不生成命令，命令部分在后续节点处理
        }
        Node::Export { name, value: _ } => {
            commands.push(vec!["export".to_string(), name.clone()]);
        }
        Node::Return { value: _ } => {
            commands.push(vec!["return".to_string()]);
        }
        // 其他节点类型（子 shell、条件语句等）——跳过内部命令
        _ => {}
    }
}

pub fn parse_shell(command: &str) -> ParsedShell {
    let lexer = Lexer::new(command);
    let mut parser = Parser::new(lexer);
    let ast = parser.parse_script();

    let (commands, has_pipe, has_multiple) = collect_commands(&ast);

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

        if exec_policy.is_empty() {
            // 无规则 = 允许全部，跳过检查
            continue;
        }

        match exec_policy.check(cmd) {
            Some((_rule, true)) => {}
            Some((rule, false)) => {
                let reason = if rule.command_name == "*" && !rule.allowed {
                    let available = exec_policy.allowed_patterns();
                    if available.is_empty() {
                        format!("禁止: {}，可用命令: 无", display_cmd(cmd))
                    } else {
                        let display: Vec<String> = available
                            .iter()
                            .map(|p| format!("`{}`", p.replace(" *", " ...")))
                            .collect();
                        format!(
                            "禁止: {}，可用命令: {}",
                            display_cmd(cmd),
                            display.join("、")
                        )
                    }
                } else {
                    format!("禁止: {}，此模式不可用: {}", display_cmd(cmd), rule.raw)
                };
                return ShellCheckResult {
                    allowed: false,
                    matched_pattern: Some(rule.raw.clone()),
                    reason: Some(reason),
                };
            }
            None => {
                return ShellCheckResult {
                    allowed: false,
                    matched_pattern: None,
                    reason: Some(format!("禁止: {}，未匹配任何允许规则", display_cmd(cmd))),
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
