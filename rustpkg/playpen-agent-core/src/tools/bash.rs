use std::sync::Arc;

use rig_core::completion::ToolDefinition;
use rig_core::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::workspace::{ValidationResult, Workspace};

#[derive(Debug, thiserror::Error)]
pub enum BashError {
    #[error("沙箱执行失败：{0}")]
    Exec(String),
    #[error("工作目录被沙箱拒绝：{0}")]
    CwdDenied(String),
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BashParams {
    #[schemars(description = "要执行的 shell 命令，严禁使用 cd 切换目录")]
    pub command: String,
    #[schemars(description = "超时时间（毫秒，可选，默认 60000）")]
    pub timeout_ms: Option<u64>,
    #[schemars(description = "工作目录（相对于项目根路径或绝对路径，可选，默认使用项目根路径）")]
    pub cwd: Option<String>,
}

pub struct BashRigTool {
    pub ws: Arc<Workspace>,
}

impl Tool for BashRigTool {
    const NAME: &'static str = "bash";
    type Error = BashError;
    type Args = BashParams;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schemars::schema_for!(BashParams)).unwrap();
        ToolDefinition {
            name: "bash".into(),
            description: "在沙箱中执行 shell 命令。支持可选的超时控制。".into(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<String, Self::Error> {
        let command = args.command;
        let cwd = match args.cwd {
            Some(ref p) => {
                let resolved = self.ws.resolve_path(p);
                if matches!(self.ws.check_path(&resolved), ValidationResult::Denied) {
                    return Err(BashError::CwdDenied(resolved.display().to_string()));
                }
                resolved
            }
            None => self.ws.project_root.clone(),
        };
        let config = Arc::clone(&self.ws.sandbox_config);

        let timeout_duration = args
            .timeout_ms
            .map(std::time::Duration::from_millis)
            .unwrap_or(std::time::Duration::from_secs(60));

        let result = tokio::time::timeout(
            timeout_duration,
            tokio::task::spawn_blocking(move || {
                crate::workspace::exec_in_sandbox(&command, &cwd, &config)
            }),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let output = output.map_err(|e| BashError::Exec(e.to_string()))?;
                let code = output.code;
                if code == 0 {
                    Ok("命令执行成功（退出码：0）".to_string())
                } else {
                    Ok(format!("命令执行完成（退出码：{})", code))
                }
            }
            Ok(Err(e)) => Err(BashError::Exec(format!("命令执行失败：{}", e))),
            Err(_elapsed) => Err(BashError::Exec(format!(
                "命令执行超时（超过 {} 毫秒）",
                timeout_duration.as_millis()
            ))),
        }
    }
}
