use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use playpen_toolkit::fetch::{FetchOption, Fetcher};

use playpen_content::{ContentBlock, Resource};

use crate::tool::{Tool, ToolContext};

pub(crate) struct WebFetchTool {
    pub(crate) web: Arc<dyn Fetcher>,
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        "获取网页内容。text/html 自动转为 Markdown。"
    }

    fn parameters_schema(&self) -> Option<Value> {
        Some(serde_json::to_value(schemars::schema_for!(FetchOption)).unwrap())
    }

    async fn execute(&self, _ctx: ToolContext, args: Value) -> anyhow::Result<Vec<ContentBlock>> {
        let opt: FetchOption = serde_json::from_value(args)?;
        let url = opt.url.clone();

        let block = match self.web.fetch(opt) {
            Ok(result) => {
                ContentBlock::resource(Resource::text(url, &result.media_type, &result.content))
            }
            Err(e) => ContentBlock::text(format!("获取网页失败: {}", e)),
        };

        Ok(vec![block])
    }
}
