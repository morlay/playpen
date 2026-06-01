use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{FileSystem, ReadOption};

use playpen_content::{ContentBlock, Resource};

use crate::tool::{Tool, ToolContext};

pub(crate) struct ReadFileTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read"
    }

    fn description(&self) -> &str {
        "读取文件内容，支持按行偏移（offset/limit）切片查看"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(ReadOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: ReadOption = serde_json::from_value(args)?;
        let path = opt.path.clone();

        let block = match self.fs.read(opt) {
            Ok(result) => {
                ContentBlock::resource(Resource::text(&path, "text/plain", &result.content))
            }
            Err(e) => ContentBlock::text(format!("读取失败: {}", e)),
        };

        Ok(vec![block])
    }
}
