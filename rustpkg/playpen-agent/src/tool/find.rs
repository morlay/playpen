use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{FileSystem, FindOption};

use playpen_content::{ContentBlock, Resource};

use crate::tool::{Tool, ToolContext};

pub(crate) struct FindTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for FindTool {
    fn name(&self) -> &str {
        "find"
    }

    fn description(&self) -> &str {
        "使用 glob 模式搜索文件，支持路径过滤和结果数量限制"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(FindOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: FindOption = serde_json::from_value(args)?;

        let block = match self.fs.find(opt) {
            Ok(files) => {
                let paths: Vec<String> = files.into_iter().map(|f| f.path).collect();
                ContentBlock::resource(Resource::text(
                    "/dev/stdout",
                    "text/plain",
                    paths.join("\n"),
                ))
            }
            Err(e) => ContentBlock::text(format!("搜索文件失败: {}", e)),
        };

        Ok(vec![block])
    }
}
