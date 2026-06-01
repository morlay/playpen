use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{FileSystem, GrepOption};

use playpen_content::{ContentBlock, Resource};

use crate::tool::{Tool, ToolContext};

pub(crate) struct GrepTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "使用正则表达式在文件内容中搜索。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(GrepOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: GrepOption = serde_json::from_value(args)?;

        let block = match self.fs.grep(opt) {
            Ok(matches) => {
                let mut lines = Vec::new();
                for m in matches {
                    for c in &m.contents {
                        lines.push(format!("{}:\n{}\n", m.path, c));
                    }
                }
                if lines.is_empty() {
                    ContentBlock::text("未找到匹配")
                } else {
                    ContentBlock::resource(Resource::text(
                        "/dev/stdout",
                        "text/plain",
                        lines.join("\n"),
                    ))
                }
            }
            Err(e) => ContentBlock::text(format!("搜索失败: {}", e)),
        };

        Ok(vec![block])
    }
}
