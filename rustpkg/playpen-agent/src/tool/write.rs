use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{FileSystem, WriteOption};

use playpen_content::ContentBlock;

use crate::tool::{Tool, ToolContext};

pub(crate) struct WriteFileTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "创建新文件或覆盖已有文件。自动创建父目录。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(WriteOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: WriteOption = serde_json::from_value(args)?;

        let block = match self.fs.write(opt) {
            Ok(result) => {
                let mut annotations = serde_json::json!({ "path": result.path });
                if let Some(ref ot) = result.old_text {
                    annotations["old_text"] = serde_json::json!(ot);
                }
                annotations["new_text"] = serde_json::json!(&result.new_text);
                ContentBlock::text("写入成功").with_annotations(annotations)
            }
            Err(e) => ContentBlock::text(format!("写入文件失败: {}", e)),
        };

        Ok(vec![block])
    }
}
