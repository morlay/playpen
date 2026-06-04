use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditParams {
    #[schemars(description = "文件路径")]
    pub path: String,
    #[schemars(description = "编辑操作列表")]
    pub edits: Vec<EditOperation>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EditOperation {
    #[schemars(description = "要被替换的原文（必须在文件中唯一出现）")]
    pub old_text: String,
    #[schemars(description = "替换后的新文本")]
    pub new_text: String,
}

pub struct EditRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for EditRigTool {
    const NAME: &'static str = "edit";
    type Error = WorkspaceError;
    type Args = EditParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(EditParams)).unwrap();
        ToolDefinition {
            name: "edit".into(),
            description: "精确替换文件内容。每个 old_text 必须在文件中唯一匹配。".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let target_path = self.ws.resolve_path(&args.path);
        let mut content = self.ws.read_file(&target_path)?;

        for (i, op) in args.edits.iter().enumerate() {
            let occurrences = content.matches(&op.old_text).count();
            if occurrences == 0 {
                return Err(WorkspaceError::Other(format!(
                    "第 {} 个编辑操作：未找到匹配文本:\n{}", i + 1, op.old_text
                )));
            }
            if occurrences > 1 {
                return Err(WorkspaceError::Other(format!(
                    "第 {} 个编辑操作：匹配文本出现 {} 次（需唯一匹配）:\n{}",
                    i + 1, occurrences, op.old_text
                )));
            }
            content = content.replacen(&op.old_text, &op.new_text, 1);
        }

        self.ws.write_file(&target_path, &content)?;
        Ok(format!("已成功编辑文件 {}，共应用 {} 个编辑操作", args.path, args.edits.len()))
    }
}
