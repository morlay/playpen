use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindParams {
    #[schemars(description = "glob 搜索模式（如 \"*.rs\"、\"**/*.toml\"）")]
    pub pattern: String,
    #[schemars(description = "搜索路径（可选，默认为项目根目录）")]
    pub path: Option<String>,
    #[schemars(description = "最大结果数（可选，默认 100）")]
    pub limit: Option<usize>,
}

pub struct FindRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for FindRigTool {
    const NAME: &'static str = "find";
    type Error = WorkspaceError;
    type Args = FindParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(FindParams)).unwrap();
        ToolDefinition {
            name: "find".into(),
            description: "使用 glob 模式搜索文件，支持路径过滤和结果数量限制".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let pattern = glob::Pattern::new(&args.pattern)
            .map_err(|e| WorkspaceError::Other(format!("无效的 glob 模式：{}，{}", args.pattern, e)))?;

        let search_path = self.ws.resolve_path(args.path.as_deref().unwrap_or("."));
        let files = self.ws.walk_files(&search_path, None)
            .map_err(|e| WorkspaceError::Other(e.to_string()))?;

        let limit = args.limit.unwrap_or(100);
        let mut results = Vec::new();

        for file_path in files {
            let name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let rel = file_path.strip_prefix(&self.ws.project_root).unwrap_or(&file_path).to_string_lossy();
            if pattern.matches(name) || pattern.matches(rel.as_ref()) {
                results.push(rel.to_string());
                if results.len() >= limit { break; }
            }
        }

        let mut output = results.join("\n");
        output.push_str(&format!("\n--- 共 {} 个文件 ---", results.len()));
        Ok(output)
    }
}
