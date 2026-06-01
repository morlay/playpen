use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, ContentBlock, ResourceLink, TextContent,
    UnstructuredCommandInput,
};
use playpen_profile::Skill;

/// DNS-RFC-1123 单标签校验。
/// 只允许字母/数字/连字符，不能以连字符开头/结尾，长度 ≤ 63。
fn is_valid_dns_label(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }
    let bytes = s.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() {
        return false;
    }
    if !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    // 单字符标签不需要检查中间部分
    if bytes.len() > 1
        && !bytes[1..bytes.len() - 1]
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return false;
    }
    true
}

/// Slash command 的类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlashCommandKind {
    Rewind,
    Skill,
}

/// 解析后的 slash command。
#[derive(Debug, Clone)]
pub(crate) struct SlashCommand {
    pub kind: SlashCommandKind,
    /// 命令名（如 "rewind"、"code"）
    pub name: String,
}

impl SlashCommand {
    pub fn builtin_rewind() -> Self {
        Self {
            kind: SlashCommandKind::Rewind,
            name: "rewind".to_string(),
        }
    }

    pub fn skill(name: String) -> Self {
        Self {
            kind: SlashCommandKind::Skill,
            name,
        }
    }
}

/// 从文本中解析 slash command，返回 `(command, args)` 或 `None`。
///
/// 规则：
/// - 文本 trim 后必须以 `/` 开头
/// - 如果文本被 `"..."` 包裹或以 `` ``` `` 开头，跳过解析
/// - `/` 后的命令名必须是 DNS-RFC-1123 标签（字母/数字/连字符）
/// - `/rewind` → Rewind，其余 → Skill
///
/// 路径如 `/foo/bar` 不会误判，因为 `foo/bar` 不是合法 DNS 标签。
pub(crate) fn parse_slash_command(text: &str) -> Option<(SlashCommand, &str)> {
    let trimmed = text.trim();

    // 跳过引号和代码块包裹的内容
    if trimmed.starts_with('"') || trimmed.starts_with("```") {
        return None;
    }

    let after_slash = trimmed.strip_prefix('/')?;

    // 命令名到空白或结尾结束
    let name_end = after_slash
        .find(char::is_whitespace)
        .unwrap_or(after_slash.len());

    if name_end == 0 {
        return None;
    }

    let name = &after_slash[..name_end];

    if !is_valid_dns_label(name) {
        return None;
    }

    let args = after_slash[name_end..].trim();

    match name {
        "rewind" => Some((SlashCommand::builtin_rewind(), args)),
        other => Some((SlashCommand::skill(other.to_string()), args)),
    }
}

/// 构建内置 rewind 的 AvailableCommand。
pub(crate) fn build_rewind_available_command() -> AvailableCommand {
    AvailableCommand::new("rewind", "回退到上一轮用户消息，重新生成回复").input(
        AvailableCommandInput::Unstructured(UnstructuredCommandInput::new("回退后的补充指令")),
    )
}

/// 构建 skill 的 AvailableCommand。
pub(crate) fn build_skill_available_command(name: &str, description: &str) -> AvailableCommand {
    AvailableCommand::new(name, description).input(AvailableCommandInput::Unstructured(
        UnstructuredCommandInput::new("技能参数"),
    ))
}

/// 处理 prompt blocks 中的 slash commands（/rewind, /{skill-name}），
/// 将包含 slash command 的 text block 拆分为对应的 blocks。
/// 返回处理后的 blocks 和是否请求了 rewind。
pub(crate) fn process_slash_commands(
    blocks: Vec<ContentBlock>,
    skills: &[Box<dyn Skill>],
) -> (Vec<ContentBlock>, bool) {
    let mut rewind_requested = false;
    let mut result = Vec::new();

    for block in blocks {
        match block {
            ContentBlock::Text(tc) => {
                let text = tc.text;

                if let Some((cmd, args)) = parse_slash_command(&text) {
                    match cmd.kind {
                        SlashCommandKind::Rewind => {
                            rewind_requested = true;
                            if !args.is_empty() {
                                result.push(ContentBlock::Text(TextContent::new(args.to_string())));
                            }
                        }
                        SlashCommandKind::Skill => {
                            if let Some(skill) =
                                skills.iter().find(|s| s.metadata().name == cmd.name)
                            {
                                let uri = format!("file://{}", skill.location().display());

                                let link = ResourceLink::new(format!("skill:{}", cmd.name), uri)
                                    .description(Some(skill.metadata().description.clone()));

                                result.push(ContentBlock::ResourceLink(link));

                                if !args.is_empty() {
                                    result.push(ContentBlock::Text(TextContent::new(
                                        args.to_string(),
                                    )));
                                }
                            } else {
                                tracing::warn!(skill = cmd.name, "skill 未找到");

                                result.push(ContentBlock::Text(TextContent::new(text)));
                            }
                        }
                    }
                } else {
                    result.push(ContentBlock::Text(TextContent::new(text)));
                }
            }
            other => {
                result.push(other);
            }
        }
    }

    (result, rewind_requested)
}

#[cfg(test)]
#[path = "slash_command_test.rs"]
mod tests;
