use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadParams {
    #[schemars(description = "文件路径")]
    pub path: String,
    #[schemars(description = "起始行号（从 1 开始，可选）")]
    pub offset: Option<usize>,
    #[schemars(description = "最大读取行数（可选）")]
    pub limit: Option<usize>,
}

pub struct ReadRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for ReadRigTool {
    const NAME: &'static str = "read";
    type Error = WorkspaceError;
    type Args = ReadParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(ReadParams)).unwrap();
        ToolDefinition {
            name: "read".into(),
            description: "读取文件内容，支持按行偏移（offset/limit）切片查看".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let target_path = self.ws.resolve_path(&args.path);
        let content = self.ws.read_file(&target_path)?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start = args.offset.unwrap_or(1).saturating_sub(1);
        let limit = args.limit.unwrap_or(usize::MAX);
        let end = (start + limit).min(total_lines);

        if start >= total_lines {
            return Ok(format!("文件 {} 共 {} 行，起始行 {} 超出范围", args.path, total_lines, args.offset.unwrap_or(1)));
        }

        let selected: Vec<&str> = lines[start..end].to_vec();
        let mut result = String::new();
        if args.offset.is_some() || args.limit.is_some() || total_lines > 100 {
            for (i, line) in selected.iter().enumerate() {
                result.push_str(&format!("{:>6}\t{}\n", start + i + 1, line));
            }
        } else {
            result = selected.join("\n");
        }
        if end < total_lines {
            result.push_str(&format!("\n... （共 {} 行，显示 {}–{}）", total_lines, start + 1, end));
        }
        Ok(result)
    }
}
