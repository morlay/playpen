use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{ValidationResult, Workspace, WorkspaceError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MoveParams {
    #[schemars(description = "源文件路径")]
    pub source: String,
    #[schemars(description = "目标路径（设为 \"/dev/null\" 表示删除）")]
    pub destination: String,
}

pub struct MoveRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for MoveRigTool {
    const NAME: &'static str = "move";
    type Error = WorkspaceError;
    type Args = MoveParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(MoveParams)).unwrap();
        ToolDefinition {
            name: "move".into(),
            description: "移动、重命名或删除文件。将 destination 设为 \"/dev/null\" 表示删除。".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let source_path = self.ws.resolve_path(&args.source);

        match self.ws.check_path(&source_path) {
            ValidationResult::Allowed => {}
            ValidationResult::ReadOnly => {
                return Err(WorkspaceError::Other(format!("源路径为只读，无法移动/删除：{}", args.source)));
            }
            ValidationResult::Denied => {
                return Err(WorkspaceError::Other(format!("源路径被沙箱拒绝：{}", args.source)));
            }
        }

        if args.destination == "/dev/null" {
            if !source_path.exists() {
                return Err(WorkspaceError::Other(format!("文件不存在：{}", args.source)));
            }
            if source_path.is_dir() {
                std::fs::remove_dir_all(source_path).map_err(|e| WorkspaceError::Io {
                    path: args.source.clone(), source: e,
                })?;
            } else {
                std::fs::remove_file(source_path).map_err(|e| WorkspaceError::Io {
                    path: args.source.clone(), source: e,
                })?;
            }
            return Ok(format!("已删除：{}", args.source));
        }

        let dest_path = self.ws.resolve_path(&args.destination);

        match self.ws.check_path(&dest_path) {
            ValidationResult::Allowed => {}
            ValidationResult::ReadOnly => {
                return Err(WorkspaceError::Other(format!("目标路径为只读，无法写入：{}", args.destination)));
            }
            ValidationResult::Denied => {
                return Err(WorkspaceError::Other(format!("目标路径被沙箱拒绝：{}", args.destination)));
            }
        }

        if let Some(parent) = dest_path.parent()
            && !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceError::Io {
                path: parent.display().to_string(), source: e,
            })?;
        }

        std::fs::rename(source_path, dest_path).map_err(|e| WorkspaceError::Io {
            path: format!("{} -> {}", args.source, args.destination), source: e,
        })?;

        Ok(format!("已移动：{} -> {}", args.source, args.destination))
    }
}
