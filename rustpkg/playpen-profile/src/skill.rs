use std::path::PathBuf;

use serde::Deserialize;

/// Skill frontmatter 元数据，直接从 YAML frontmatter 解析
#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    // DNS Subdomain Names  RFC 1123 Label Names
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, rename = "disable-model-invocation")]
    pub disable_model_invocation: Option<bool>,
}

// 文件格式
//
// ```md {name=SKILL.md}
// ---
// name: string
// description: string
// disable-model-invocation?: bool
// ---
//
// {instructions}
// ```
pub trait Skill: Send + Sync {
    fn metadata(&self) -> &Metadata;
    fn location(&self) -> &PathBuf;
    fn source(&self) -> Source;
    fn instructions(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Global,
    Project,
}

/// 从 SKILL.md 内容中提取 YAML frontmatter 并解析为 Metadata。
/// 返回 (Metadata, instructions) 其中 instructions 为 frontmatter 之后的部分。
///
/// 如果 frontmatter 不存在或解析失败，返回 None。
fn parse_skill(raw: &str) -> Option<(Metadata, &str)> {
    let raw = raw.trim();
    let rest = raw.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let body = rest[end + 4..].trim();

    let meta: Metadata = serde_yaml::from_str(frontmatter).ok()?;
    Some((meta, body))
}

/// 从磁盘读取 SKILL.md 文件并解析。
fn parse_skill_file(path: &PathBuf) -> Option<(Metadata, String)> {
    let raw = std::fs::read_to_string(path).ok()?;
    let (meta, body) = parse_skill(&raw)?;
    Some((meta, body.to_string()))
}

/// Skill 的本地文件实现。
#[derive(Debug, Clone)]
pub struct LocalSkill {
    metadata: Metadata,
    location: PathBuf,
    source: Source,
    instructions: String,
}

impl LocalSkill {
    pub fn new(
        metadata: Metadata,
        location: PathBuf,
        source: Source,
        instructions: String,
    ) -> Self {
        Self {
            metadata,
            location,
            source,
            instructions,
        }
    }

    /// 从 SKILL.md 文件加载并解析。
    pub fn load(path: PathBuf, source: Source) -> Option<Self> {
        let (metadata, instructions) = parse_skill_file(&path)?;
        Some(Self {
            metadata,
            location: path,
            source,
            instructions,
        })
    }
}

impl Skill for LocalSkill {
    fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    fn location(&self) -> &PathBuf {
        &self.location
    }

    fn source(&self) -> Source {
        self.source
    }

    fn instructions(&self) -> &str {
        &self.instructions
    }
}

#[cfg(test)]
#[path = "skill_test.rs"]
mod tests;
