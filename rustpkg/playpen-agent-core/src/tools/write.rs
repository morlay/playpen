use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WriteParams {
    #[schemars(description = "文件路径")]
    pub path: String,
    #[schemars(description = "文件内容")]
    pub content: String,
}

pub struct WriteRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for WriteRigTool {
    const NAME: &'static str = "write";
    type Error = WorkspaceError;
    type Args = WriteParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(WriteParams)).unwrap();
        ToolDefinition {
            name: "write".into(),
            description: "创建新文件或覆盖已有文件。自动创建父目录。".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let target_path = self.ws.resolve_path(&args.path);

        // 确保父目录存在
        if let Some(parent) = target_path.parent()
            && !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }

        self.ws.write_file(&target_path, &args.content)?;
        Ok(format!("已成功写入文件 {}（{} 字节）", args.path, args.content.len()))
    }
}
