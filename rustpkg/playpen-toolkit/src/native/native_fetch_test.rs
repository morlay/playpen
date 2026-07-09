use super::*;
use crate::fetch::{FetchOption, Fetcher};

#[test]
fn fetch_text() {
    let server = httpmock::MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET").path("/data");
        then.status(200)
            .header("Content-Type", "text/plain")
            .body("hello from web");
    });

    let fetcher = NativeFetcher;
    let result = fetcher
        .fetch(FetchOption {
            url: server.url("/data"),
            timeout_ms: None,
            max_bytes: None,
            accept: None,
        })
        .unwrap();

    assert_eq!(result.content, "hello from web");
    mock.assert_hits(1);
}

#[test]
fn fetch_not_found() {
    let server = httpmock::MockServer::start();
    server.mock(|when, then| {
        when.method("GET").path("/missing");
        then.status(404);
    });

    let fetcher = NativeFetcher;
    let result = fetcher.fetch(FetchOption {
        url: server.url("/missing"),
        timeout_ms: None,
        max_bytes: None,
        accept: None,
    });
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.downcast_ref::<crate::fetch::FetchError>().is_some());
}

#[test]
fn fetch_html_to_markdown() {
    let server = httpmock::MockServer::start();
    server.mock(|when, then| {
        when.method("GET").path("/page");
        then.status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body("<html><body><h1>Title</h1><p>Hello <strong>World</strong></p></body></html>");
    });

    let fetcher = NativeFetcher;
    let result = fetcher
        .fetch(FetchOption {
            url: server.url("/page"),
            timeout_ms: None,
            max_bytes: None,
            accept: None,
        })
        .unwrap();

    assert!(
        result.content.contains("Title"),
        "应包含标题: {}",
        result.content
    );
    assert!(
        result.content.contains("Hello"),
        "应包含段落: {}",
        result.content
    );
    assert!(
        result.content.contains("**World**") || result.content.contains("World"),
        "应包含加粗文本: {}",
        result.content
    );
}

#[test]
fn fetch_invalid_url() {
    let fetcher = NativeFetcher;
    let result = fetcher.fetch(FetchOption {
        url: "http://invalid.invalid/.invalid".into(),
        timeout_ms: Some(1000),
        max_bytes: None,
        accept: None,
    });
    assert!(result.is_err());
}

// ---- html_to_markdown 单元测试 ----

#[test]
fn strips_script_tags() {
    let html = r#"
        <h1>Hello</h1>
        <script>alert('xss')</script>
        <p>world</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("Hello"), "应保留标题内容: {}", md);
    assert!(md.contains("world"), "应保留段落内容: {}", md);
    assert!(!md.contains("alert"), "不应包含脚本代码: {}", md);
    assert!(!md.contains("xss"), "不应包含脚本代码: {}", md);
}

#[test]
fn strips_style_tags() {
    let html = r#"
        <p>content</p>
        <style>body { color: red; }</style>
        <p>more</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("content"), "应保留内容: {}", md);
    assert!(md.contains("more"), "应保留内容: {}", md);
    assert!(!md.contains("color: red"), "不应包含 CSS: {}", md);
    assert!(!md.contains("body {"), "不应包含 CSS: {}", md);
}

#[test]
fn strips_noscript_tags() {
    let html = r#"
        <p>text</p>
        <noscript>your browser does not support JavaScript</noscript>
        <p>end</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("text"), "应保留内容: {}", md);
    assert!(md.contains("end"), "应保留内容: {}", md);
    assert!(!md.contains("JavaScript"), "不应包含 noscript 内容: {}", md);
}

#[test]
fn strips_svg_tags() {
    let html = r#"
        <p>content</p>
        <svg><circle cx="50" cy="50" r="40" /></svg>
        <p>end</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("content"), "应保留内容: {}", md);
    assert!(md.contains("end"), "应保留内容: {}", md);
    assert!(!md.contains("circle"), "不应包含 SVG 内容: {}", md);
}

#[test]
fn strips_template_tags() {
    let html = r#"
        <p>content</p>
        <template><h1>hidden</h1></template>
        <p>end</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("content"), "应保留内容: {}", md);
    assert!(md.contains("end"), "应保留内容: {}", md);
    assert!(!md.contains("hidden"), "不应包含 template 内容: {}", md);
}

#[test]
fn strips_iframe_tags() {
    let html = r#"
        <p>content</p>
        <iframe src="https://example.com"></iframe>
        <p>end</p>
    "#;
    let md = html_to_markdown(html);
    assert!(md.contains("content"), "应保留内容: {}", md);
    assert!(md.contains("end"), "应保留内容: {}", md);
    assert!(!md.contains("iframe"), "不应包含 iframe: {}", md);
}

#[test]
fn converts_headings() {
    let html = "<h1>A</h1><h2>B</h2><h3>C</h3>";
    let md = html_to_markdown(html);
    assert!(md.contains("# A"), "h1 应转为一阶标题: {}", md);
    assert!(md.contains("## B"), "h2 应为二阶标题: {}", md);
    assert!(md.contains("### C"), "h3 应为三阶标题: {}", md);
}

#[test]
fn converts_links() {
    let html = r#"<a href="https://example.com">Example</a>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains("[Example](https://example.com)"),
        "链接格式错误: {}",
        md
    );
}

#[test]
fn converts_images() {
    let html = r#"<img src="cat.png" alt="A cat" />"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains("![A cat](cat.png)"),
        "图片格式错误: {}",
        md
    );
}

#[test]
fn converts_code_block() {
    let html = "<pre><code>fn main() {}\n</code></pre>";
    let md = html_to_markdown(html);
    assert!(md.contains("```"), "应生成代码块: {}", md);
    assert!(md.contains("fn main()"), "应保留代码内容: {}", md);
}

#[test]
fn converts_inline_code() {
    let html = r#"<p>use <code>std::fmt</code></p>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains("`std::fmt`"),
        "内联代码格式错误: {}",
        md
    );
}

#[test]
fn converts_unordered_list() {
    let html = "<ul><li>A</li><li>B</li></ul>";
    let md = html_to_markdown(html);
    assert!(md.contains("- A"), "应包含无序列表项: {}", md);
    assert!(md.contains("- B"), "应包含无序列表项: {}", md);
}

#[test]
fn converts_ordered_list() {
    let html = "<ol><li>First</li><li>Second</li></ol>";
    let md = html_to_markdown(html);
    assert!(md.contains("1. First"), "应包含有序列表项: {}", md);
    assert!(md.contains("1. Second"), "应包含有序列表项: {}", md);
}

#[test]
fn converts_blockquote() {
    let html = "<blockquote><p>quote text</p></blockquote>";
    let md = html_to_markdown(html);
    assert!(md.contains("> quote text"), "引用格式错误: {}", md);
}

#[test]
fn converts_bold_and_italic() {
    let html = "<p><strong>Bold</strong> and <em>Italic</em></p>";
    let md = html_to_markdown(html);
    assert!(md.contains("**Bold**"), "加粗格式错误: {}", md);
    assert!(md.contains("*Italic*"), "斜体格式错误: {}", md);
}

#[test]
fn converts_horizontal_rule() {
    let html = "<p>A</p><hr /><p>B</p>";
    let md = html_to_markdown(html);
    assert!(md.contains("---"), "应生成水平线: {}", md);
}

#[test]
fn complex_html_with_noise() {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>Test Page</title>
            <style>body { margin: 0; }</style>
        </head>
        <body>
            <header><h1>My Site</h1></header>
            <nav><a href="/">Home</a> <a href="/about">About</a></nav>
            <article>
                <h2>Article Title</h2>
                <p>This is <strong>important</strong> content.</p>
                <script>
                    console.log("tracking");
                </script>
                <p>More content here.</p>
            </article>
            <footer>&copy; 2025</footer>
        </body>
        </html>
    "#;
    let md = html_to_markdown(html);

    // 应保留的内容
    assert!(md.contains("My Site"), "应保留 header 标题: {}", md);
    assert!(md.contains("Article Title"), "应保留文章标题: {}", md);
    assert!(md.contains("important"), "应保留加粗内容: {}", md);
    assert!(md.contains("More content"), "应保留更多内容: {}", md);

    // 不应包含的噪声
    assert!(!md.contains("tracking"), "不应包含脚本内容: {}", md);
    assert!(!md.contains("margin: 0"), "不应包含样式内容: {}", md);
    assert!(!md.contains("console.log"), "不应包含脚本内容: {}", md);
}

#[test]
fn empty_html() {
    let md = html_to_markdown("");
    assert!(md.is_empty(), "空 HTML 应返回空字符串");
}

#[test]
fn html_with_only_noise() {
    let html = "<script>bad</script><style>bad</style>";
    let md = html_to_markdown(html);
    assert!(md.is_empty(), "只有噪声标签时应返回空字符串: {}", md);
}

// ---- 转义处理测试 ----

#[test]
fn inline_code_escapes_backtick() {
    let html = r#"<p>use <code>Option::`variant</code></p>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains("``"),
        "含反引号的内容需用双反引号包裹: {}",
        md
    );
    assert!(
        md.contains("`variant"),
        "内容中的反引号应保留: {}",
        md
    );
}

#[test]
fn link_escapes_brackets_in_text() {
    let html = r#"<a href="https://example.com">[click] here</a>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains(r#"\[click\] here"#),
        "链接文本中的方括号需转义: {}",
        md
    );
}

#[test]
fn link_escapes_parens_in_url() {
    let html = r#"<a href="https://en.wikipedia.org/wiki/Rust_(programming_language)">Rust</a>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains(r#"\(programming_language\)"#),
        "URL 中的圆括号需转义: {}",
        md
    );
}

#[test]
fn img_escapes_parens_in_src() {
    let html = r#"<img src="cat(1).png" alt="cat" />"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains(r#"cat\(1\).png"#),
        "图片 src 中的圆括号需转义: {}",
        md
    );
}

#[test]
fn img_escapes_brackets_in_alt() {
    let html = r#"<img src="cat.png" alt="[best] cat" />"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains(r#"\[best\] cat"#),
        "alt 文本中的方括号需转义: {}",
        md
    );
}

#[test]
fn code_block_escapes_fence() {
    let html = "<pre><code>```\ncode block\n```\n</code></pre>";
    let md = html_to_markdown(html);
    assert!(
        md.contains("````"),
        "含三个反引号的代码块需用四个反引号围栏: {}",
        md
    );
}

#[test]
fn link_with_empty_text_escapes_url() {
    let html = r#"<a href="fn(x)"></a>"#;
    let md = html_to_markdown(html);
    assert!(
        md.contains(r#"fn\(x\)"#),
        "空文本链接的 URL 也需转义: {}",
        md
    );
}
