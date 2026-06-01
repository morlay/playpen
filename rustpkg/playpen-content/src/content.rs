use serde::{Deserialize, Serialize};

// ── ContentBlock types ───────────────────────────────────────────────────

/// Tool result output block. All variants support optional `annotations`
/// for attaching metadata (e.g. tool provenance, file paths).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text(TextContent),
    #[serde(rename = "resource")]
    Resource(Resource),
    #[serde(rename = "resource_link")]
    ResourceLink(ResourceLink),
}

// ── Text ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub annotations: Option<serde_json::Value>,
}

impl TextContent {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            annotations: None,
        }
    }

    pub fn with_annotations(mut self, annotations: serde_json::Value) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

// ── Resource ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Resource {
    Text {
        uri: String,
        media_type: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        annotations: Option<serde_json::Value>,
    },
    Blob {
        uri: String,
        media_type: String,
        #[serde(with = "base64_serde")]
        blob: Vec<u8>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        annotations: Option<serde_json::Value>,
    },
}

impl Resource {
    pub fn text(
        uri: impl Into<String>,
        media_type: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self::Text {
            uri: uri.into(),
            media_type: media_type.into(),
            text: text.into(),
            annotations: None,
        }
    }

    pub fn blob(uri: impl Into<String>, media_type: impl Into<String>, blob: Vec<u8>) -> Self {
        Self::Blob {
            uri: uri.into(),
            media_type: media_type.into(),
            blob,
            annotations: None,
        }
    }

    pub fn with_annotations(mut self, annotations: serde_json::Value) -> Self {
        match &mut self {
            Self::Text { annotations: a, .. } | Self::Blob { annotations: a, .. } => {
                *a = Some(annotations);
            }
        }
        self
    }
}

// ── ResourceLink ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLink {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub annotations: Option<serde_json::Value>,
}

impl ResourceLink {
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            media_type: None,
            size: None,
            annotations: None,
        }
    }

    pub fn with_media_type(mut self, media_type: impl Into<String>) -> Self {
        self.media_type = Some(media_type.into());
        self
    }

    pub fn with_annotations(mut self, annotations: serde_json::Value) -> Self {
        self.annotations = Some(annotations);
        self
    }
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text(TextContent::new(text))
    }

    pub fn resource(resource: Resource) -> Self {
        ContentBlock::Resource(resource)
    }

    pub fn resource_link(link: ResourceLink) -> Self {
        ContentBlock::ResourceLink(link)
    }

    pub fn with_annotations(self, annotations: serde_json::Value) -> Self {
        match self {
            ContentBlock::Text(t) => ContentBlock::Text(t.with_annotations(annotations)),
            ContentBlock::Resource(r) => ContentBlock::Resource(r.with_annotations(annotations)),
            ContentBlock::ResourceLink(l) => {
                ContentBlock::ResourceLink(l.with_annotations(annotations))
            }
        }
    }
}

impl From<String> for ContentBlock {
    fn from(text: String) -> Self {
        ContentBlock::Text(TextContent::new(text))
    }
}

impl From<&str> for ContentBlock {
    fn from(text: &str) -> Self {
        ContentBlock::Text(TextContent::new(text))
    }
}

// ── Formatting ───────────────────────────────────────────────────────────

/// 将 ContentBlock 格式化为 LLM 可见的文本。
///
/// 格式规则：
/// - Text → 纯文本
/// - Resource:Text → ```<ext> {uri="<value>"}\n{text}\n```
/// - Resource:Blob → ```<ext> {uri="<value>" mime="<media_type>,base64"}\n<base64>\n```
/// - ResourceLink → [<name>](<uri>){mime=<media_type>}
pub fn format_content_block(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(t) => t.text.clone(),

        ContentBlock::Resource(r) => match r {
            Resource::Text {
                uri,
                media_type,
                text,
                ..
            } => {
                let ext = mime_to_ext(media_type);

                format!(
                    "```{ext} {{uri={uri:?}}}\n{text}{newline}```",
                    ext = ext,
                    uri = uri,
                    text = text,
                    newline = if text.ends_with('\n') { "" } else { "\n" }
                )
            }
            Resource::Blob {
                uri,
                media_type,
                blob,
                ..
            } => {
                let ext = mime_to_ext(media_type);
                let b64 = base64_encode(blob);
                let header = if media_type.is_empty() {
                    format!("{{uri={uri:?} base64}}", uri = uri)
                } else {
                    format!(
                        "{{uri={uri:?} type={media_type:?} base64}}",
                        uri = uri,
                        media_type = media_type
                    )
                };
                format!(
                    "```{ext} {header}\n{data}\n```",
                    ext = ext,
                    header = header,
                    data = b64,
                )
            }
        },

        ContentBlock::ResourceLink(link) => {
            if let Some(ref media_type) = link.media_type {
                format!(
                    "[{name}]({uri}){{type={media_type}}}",
                    name = link.name,
                    uri = link.uri,
                    media_type = media_type
                )
            } else {
                format!("[{name}]({uri})", name = link.name, uri = link.uri)
            }
        }
    }
}

/// MIME type 转文件扩展名。
fn mime_to_ext(mime: &str) -> &str {
    if mime == "text/plain" {
        return "";
    }
    mime_guess::get_mime_extensions_str(mime)
        .and_then(|exts| exts.first().copied())
        .unwrap_or("")
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Serde helper: Vec<u8> 以 base64 字符串序列化/反序列化
mod base64_serde {
    use base64::Engine;
    use serde::Deserialize;

    pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        base64::engine::general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "content_test.rs"]
mod tests;
