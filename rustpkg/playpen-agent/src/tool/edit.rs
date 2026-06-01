use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fs::{EditOp, EditOption, FileSystem};

use playpen_content::ContentBlock;

use crate::tool::{Tool, ToolContext};

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct EditArgs {
    path: String,
    edits: Vec<EditOpArg>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
struct EditOpArg {
    old_text: String,
    new_text: String,
}

pub(crate) struct EditFileTool {
    pub(crate) fs: Arc<dyn FileSystem>,
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "精确替换文件内容。每个 old_text 必须在文件中唯一匹配。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(EditArgs)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let ea: EditArgs = serde_json::from_value(args)?;

        let ops: Vec<EditOp> = ea
            .edits
            .into_iter()
            .map(|o| EditOp {
                old_text: o.old_text,
                new_text: o.new_text,
            })
            .collect();

        let block = match self.fs.edit(EditOption {
            path: ea.path,
            edits: ops,
        }) {
            Ok(result) => {
                let diffs: Vec<serde_json::Value> = result
                    .ops
                    .iter()
                    .map(|op| {
                        serde_json::json!({
                            "old_text": op.old_text,
                            "new_text": op.new_text,
                        })
                    })
                    .collect();

                ContentBlock::text(format!("编辑成功，替换了 {} 处", result.ops.len()))
                    .with_annotations(serde_json::json!({
                        "diffs": diffs,
                        "path": result.path,
                    }))
            }
            Err(e) => ContentBlock::text(format!("编辑失败: {}", e)),
        };

        Ok(vec![block])
    }
}
