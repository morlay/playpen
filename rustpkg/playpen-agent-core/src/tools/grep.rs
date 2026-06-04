use std::sync::Arc;

use grep::searcher::{Searcher, SearcherBuilder, Sink, SinkMatch};
use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepParams {
    #[schemars(description = "正则表达式或字面字符串")]
    pub pattern: String,
    #[schemars(description = "搜索目录或文件（可选，默认为当前目录）")]
    pub path: Option<String>,
    #[schemars(description = "文件过滤 glob，如 \"*.rs\"（可选）")]
    pub glob: Option<String>,
    #[schemars(description = "忽略大小写（可选，默认 false）")]
    pub ignore_case: Option<bool>,
}

/// 收集匹配行的 Sink 实现
struct LineCollector {
    results: Vec<(u64, String)>,
}

impl Sink for LineCollector {
    type Error = std::io::Error;

    fn matched(
        &mut self,
        _searcher: &Searcher,
        mat: &SinkMatch<'_>,
    ) -> std::result::Result<bool, Self::Error> {
        let line = std::str::from_utf8(mat.bytes())
            .unwrap_or("")
            .trim_end()
            .to_string();
        self.results
            .push((mat.line_number().unwrap_or(0), line));
        Ok(true)
    }
}

pub struct GrepRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for GrepRigTool {
    const NAME: &'static str = "grep";
    type Error = WorkspaceError;
    type Args = GrepParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(GrepParams)).unwrap();
        ToolDefinition {
            name: "grep".into(),
            description: "使用正则表达式在文件内容中搜索。支持文件过滤和忽略大小写。".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let search_path = self.ws.resolve_path(args.path.as_deref().unwrap_or("."));

        let mut builder = grep::regex::RegexMatcherBuilder::new();
        if args.ignore_case.unwrap_or(false) {
            builder.case_insensitive(true);
        }
        let matcher = builder
            .build(&args.pattern)
            .map_err(|e| WorkspaceError::Other(format!("无效的正则表达式：{}，{}", args.pattern, e)))?;

        let files = self.ws.walk_files(&search_path, args.glob.as_deref())
            .map_err(|e| WorkspaceError::Other(e.to_string()))?;

        let mut searcher = SearcherBuilder::new().build();
        let mut results: Vec<(String, u64, String)> = Vec::new();
        let cwd = &self.ws.project_root;

        for file_path in files {
            let display = file_path.strip_prefix(cwd).unwrap_or(&file_path);
            let mut collector = LineCollector {
                results: Vec::new(),
            };
            let content = std::fs::read_to_string(&file_path).map_err(|e| WorkspaceError::Io {
                path: file_path.display().to_string(),
                source: e,
            })?;
            searcher.search_slice(&matcher, content.as_bytes(), &mut collector)
                .map_err(|e| WorkspaceError::Other(e.to_string()))?;
            for (line_no, line) in collector.results {
                results.push((display.to_string_lossy().to_string(), line_no, line));
            }
        }

        let mut output = String::new();
        for (path, line_no, line) in &results {
            output.push_str(&format!("{}:{}: {}\n", path, line_no, line));
        }
        output.push_str(&format!("\n--- 共 {} 个匹配 ---", results.len()));
        Ok(output)
    }
}
