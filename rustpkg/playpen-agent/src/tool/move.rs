use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{FileSystem, MoveOption};

use playpen_content::ContentBlock;

use crate::tool::{Tool, ToolContext};

pub(crate) struct MoveFileTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for MoveFileTool {
    fn name(&self) -> &str {
        "move"
    }

    fn description(&self) -> &str {
        "移动、重命名或删除文件。new_path 设为 \"/dev/null\" 表示删除。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(MoveOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: MoveOption = serde_json::from_value(args)?;

        let block = match self.fs.r#move(opt) {
            Ok(result) => {
                let message = if result.deleted == Some(true) {
                    "删除成功"
                } else {
                    "移动成功"
                };
                ContentBlock::text(message)
            }
            Err(e) => ContentBlock::text(format!("移动/删除文件失败: {}", e)),
        };

        Ok(vec![block])
    }
}
