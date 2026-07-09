use std::time::Duration;

use crate::fetch::{FetchError, FetchOption, FetchResult, Fetcher};
use scraper::{ElementRef, Html, Node};

pub struct NativeFetcher;

impl Fetcher for NativeFetcher {
    fn fetch(&self, opt: FetchOption) -> anyhow::Result<FetchResult> {
        let timeout = Duration::from_millis(opt.timeout_ms.unwrap_or(30000));
        let max_bytes = opt.max_bytes.unwrap_or(10 * 1024 * 1024);

        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .user_agent("playpen-agent/1.0")
            .build()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let response = client
            .get(&opt.url)
            .send()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::HttpStatus(format!("HTTP {}", status.as_u16())).into());
        }

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response
            .bytes()
            .map_err(|e| FetchError::Network(e.to_string()))?;

        let bytes = if bytes.len() > max_bytes {
            bytes.slice(..max_bytes)
        } else {
            bytes
        };

        let is_html = content_type.starts_with("text/html");
        let want_html = opt
            .accept
            .as_deref()
            .is_none_or(|ct| ct.is_empty() || ct == "text/html");

        if is_html && want_html {
            let html = String::from_utf8_lossy(&bytes).to_string();
            let md = html_to_markdown(&html);
            Ok(FetchResult {
                content: md,
                media_type: "text/markdown".into(),
            })
        } else if content_type.contains("charset") || content_type.starts_with("text/") {
            Ok(FetchResult {
                content: String::from_utf8_lossy(&bytes).to_string(),
                media_type: content_type,
            })
        } else {
            Ok(FetchResult {
                content: format!(
                    "[非文本内容，Content-Type: {content_type}，{} 字节]",
                    bytes.len()
                ),
                media_type: content_type,
            })
        }
    }
}

/// 将 HTML 转换为 Markdown，自动忽略 script、style 等噪声标签。
fn html_to_markdown(html: &str) -> String {
    let fragment = Html::parse_fragment(html);
    let mut md = String::new();
    let ctx = ConvertCtx::default();
    process_children(&fragment.root_element(), &mut md, &ctx);

    // 清理：合并超过 2 个的连续换行
    let re = regex::Regex::new(r"\n{3,}").unwrap();
    let md = re.replace_all(&md, "\n\n").into_owned();
    md.trim().to_string()
}

#[derive(Default, Clone)]
struct ConvertCtx {
    in_pre: bool,
    ordered: bool,
}

fn process_children(parent: &ElementRef, md: &mut String, ctx: &ConvertCtx) {
    for child in parent.children() {
        match child.value() {
            Node::Text(text) => {
                if ctx.in_pre {
                    md.push_str(&text);
                } else {
                    let text = text.trim();
                    if !text.is_empty() {
                        if !md.is_empty() && !md.ends_with(' ') && !md.ends_with('\n') {
                            md.push(' ');
                        }
                        md.push_str(text);
                    }
                }
            }
            Node::Element(_) => {
                if let Some(el) = ElementRef::wrap(child) {
                    process_element(&el, md, ctx);
                }
            }
            _ => {}
        }
    }
}

fn process_element(el: &ElementRef, md: &mut String, ctx: &ConvertCtx) {
    let tag = el.value().name();

    match tag {
        // === 完全忽略的标签 ===
        "script" | "style" | "noscript" | "svg" | "template" | "iframe" => {}

        // === 标题 ===
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = tag.as_bytes()[1] - b'0';
            ensure_newline(md);
            for _ in 0..level {
                md.push('#');
            }
            md.push(' ');
            // 标题内部只取纯文本
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            md.push_str(text.trim());
            md.push('\n');
        }

        // === 段落 ===
        "p" => {
            ensure_newline(md);
            process_children(el, md, ctx);
            if !md.ends_with('\n') {
                md.push('\n');
            }
        }

        // === 换行 / 水平线 ===
        "br" => {
            md.push('\n');
        }
        "hr" => {
            ensure_newline(md);
            md.push_str("---\n");
        }

        // === 行内样式 ===
        "b" | "strong" => {
            wrap_inline(el, md, ctx, "**", "**");
        }
        "i" | "em" => {
            wrap_inline(el, md, ctx, "*", "*");
        }
        "s" | "del" => {
            wrap_inline(el, md, ctx, "~~", "~~");
        }
        "u" | "ins" => {
            wrap_inline(el, md, ctx, "__", "__");
        }

        // === 代码 ===
        "code" => {
            if ctx.in_pre {
                // 在 <pre> 内部，<code> 只是容器，内容由 <pre> 处理
                process_children(el, md, ctx);
            } else {
                let text: String = el.text().collect();
                md.push_str(&inline_code_marker(text.trim()));
            }
        }
        "pre" => {
            let mut child_ctx = ctx.clone();
            child_ctx.in_pre = true;
            let mut inner = String::new();
            process_children(el, &mut inner, &child_ctx);
            ensure_newline(md);
            md.push_str(&fence_code_block(&inner));
        }

        // === 链接 ===
        "a" => {
            let href = el.value().attr("href").unwrap_or("");
            let text: String = el.text().collect::<Vec<_>>().join(" ");
            let text = text.trim();
            if !text.is_empty() {
                md.push('[');
                md.push_str(&escape_link_text(text));
                md.push(']');
                md.push('(');
                md.push_str(&escape_url(href));
                md.push(')');
            } else if !href.is_empty() {
                md.push_str(&escape_url(href));
            }
        }

        // === 图片 ===
        "img" => {
            let src = el.value().attr("src").unwrap_or("");
            let alt = el.value().attr("alt").unwrap_or("");
            md.push_str("![");
            md.push_str(&escape_link_text(alt));
            md.push_str("](");
            md.push_str(&escape_url(src));
            md.push(')');
        }

        // === 列表 ===
        "ul" => {
            let mut child_ctx = ctx.clone();
            child_ctx.ordered = false;
            ensure_newline(md);
            process_children(el, md, &child_ctx);
            md.push('\n');
        }
        "ol" => {
            let mut child_ctx = ctx.clone();
            child_ctx.ordered = true;
            ensure_newline(md);
            process_children(el, md, &child_ctx);
            md.push('\n');
        }
        "li" => {
            let child_ctx = ctx.clone();
            let mut item_text = String::new();
            process_children(el, &mut item_text, &child_ctx);

            md.push('\n');
            if ctx.ordered {
                // 所有有序列表项统一用 1.，Markdown 渲染器会自动编号
                md.push_str("1. ");
            } else {
                md.push_str("- ");
            }
            md.push_str(item_text.trim());
        }

        // === 引用 ===
        "blockquote" => {
            ensure_newline(md);
            let mut inner = String::new();
            process_children(el, &mut inner, ctx);
            for line in inner.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    md.push_str("> ");
                    md.push_str(trimmed);
                }
                md.push('\n');
            }
        }

        // === 容器：只需递归子节点 ===
        "div" | "section" | "article" | "main" | "header" | "footer" | "nav" | "aside"
        | "span" | "html" | "head" | "body" => {
            process_children(el, md, ctx);
        }

        // === 表格：简单提取文本 ===
        "table" | "thead" | "tbody" | "tr" | "td" | "th" => {
            // 简单提取，不做表格格式
            process_children(el, md, ctx);
            if matches!(tag, "tr" | "table") {
                md.push('\n');
            }
        }

        // === 默认：递归子节点 ===
        _ => {
            process_children(el, md, ctx);
        }
    }
}

/// 用前后标记包装元素内部的文本
fn wrap_inline(el: &ElementRef, md: &mut String, ctx: &ConvertCtx, open: &str, close: &str) {
    let mut inner = String::new();
    process_children(el, &mut inner, ctx);
    let inner = inner.trim();
    if !inner.is_empty() {
        md.push_str(open);
        md.push_str(inner);
        md.push_str(close);
    }
}

/// 用正确的反引号数量包裹行内代码，避免内容中的反引号与分隔符冲突。
fn inline_code_marker(content: &str) -> String {
    if content.contains('`') {
        // 找内容中最长连续反引号运行长度
        let max_run = content
            .chars()
            .fold((0usize, 0usize), |(max, cur), c| {
                if c == '`' {
                    let new_cur = cur + 1;
                    (max.max(new_cur), new_cur)
                } else {
                    (max, 0)
                }
            })
            .0;
        let count = max_run + 1;
        let ticks = "`".repeat(count);
        format!("{ticks} {content} {ticks}")
    } else {
        format!("`{content}`")
    }
}

/// 用正确的围栏包裹代码块，避免内容中的反引号与围栏冲突。
fn fence_code_block(content: &str) -> String {
    let content = content.trim();
    if content.contains("```") {
        // 内容包含三个反引号，用四个反引号作为围栏
        format!("````\n{content}\n````\n")
    } else if content.contains("``") {
        format!("```\n{content}\n```\n")
    } else {
        format!("```\n{content}\n```\n")
    }
}

/// 转义链接文本中的 Markdown 特殊字符 `[` `]`。
fn escape_link_text(text: &str) -> String {
    text.replace('[', r"\[").replace(']', r"\]")
}

/// 转义 URL 中的 Markdown 特殊字符 `(` `)`，防止与链接语法冲突。
fn escape_url(url: &str) -> String {
    url.replace('(', r"\(").replace(')', r"\)")
}

/// 确保前一个内容后有换行
fn ensure_newline(md: &mut String) {
    if !md.is_empty() && !md.ends_with('\n') {
        md.push('\n');
    }
}

#[cfg(test)]
#[path = "native_fetch_test.rs"]
mod native_fetch_test;
