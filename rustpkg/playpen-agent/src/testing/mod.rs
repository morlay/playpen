//! Test utilities for playpen-agent.
//!
//! Re-exports rig's `test_utils` and provides reusable helpers.
pub use rig_core::test_utils::{MockCompletionModel, MockStreamEvent};

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use playpen_config::Settings;
use playpen_config::model::ModelProfile;
use playpen_content::ContentBlock;

use crate::runner::SimpleRunner;
use crate::tool::{Tool, ToolContext};

/// 模拟工具，返回固定结果。
pub struct FakeTool {
    pub name: String,
    pub blocks: Vec<ContentBlock>,
}

#[async_trait]
impl Tool for FakeTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "fake tool for testing"
    }
    fn parameters_schema(&self) -> Option<serde_json::Value> {
        None
    }
    async fn execute(
        &self,
        _ctx: ToolContext,
        _args: serde_json::Value,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        Ok(self.blocks.clone())
    }
}

impl FakeTool {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            blocks: vec![ContentBlock::text(text)],
        }
    }
}

/// 测试用 AgentProfile。
pub struct TestProfile;

impl playpen_profile::AgentProfile for TestProfile {
    fn name(&self) -> &str {
        "test"
    }
    fn description(&self) -> Option<&str> {
        None
    }
    fn working_dir(&self) -> &PathBuf {
        static TMP: std::sync::LazyLock<PathBuf> =
            std::sync::LazyLock::new(|| PathBuf::from("/tmp"));
        &TMP
    }
    fn model_profile(&self) -> &ModelProfile {
        static MP: ModelProfile = ModelProfile {
            model: String::new(),
            temperature: None,
            top_p: None,
            thinking_level: None,
        };
        &MP
    }
    fn instructions(&self) -> anyhow::Result<String> {
        Ok("You are a test assistant.".into())
    }
    fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::Skill>>> {
        Ok(vec![])
    }
    fn tool_enabled(&self, name: &str) -> bool {
        name == "test_tool"
    }
    fn with_model_profile(
        &self,
        f: &dyn Fn(&ModelProfile) -> ModelProfile,
    ) -> Box<dyn playpen_profile::AgentProfile> {
        let mp = f(self.model_profile());
        struct P(Box<dyn playpen_profile::AgentProfile>, ModelProfile);
        impl playpen_profile::AgentProfile for P {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn description(&self) -> Option<&str> {
                None
            }
            fn working_dir(&self) -> &PathBuf {
                self.0.working_dir()
            }
            fn model_profile(&self) -> &ModelProfile {
                &self.1
            }
            fn instructions(&self) -> anyhow::Result<String> {
                self.0.instructions()
            }
            fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::Skill>>> {
                self.0.available_skills()
            }
            fn tool_enabled(&self, n: &str) -> bool {
                self.0.tool_enabled(n)
            }
            fn with_model_profile(
                &self,
                f: &dyn Fn(&ModelProfile) -> ModelProfile,
            ) -> Box<dyn playpen_profile::AgentProfile> {
                self.0.with_model_profile(f)
            }
        }
        Box::new(P(Box::new(TestProfile), mp))
    }
}

/// 创建测试用 SimpleRunner。
pub async fn make_runner(
    session: Box<dyn playpen_session::Session>,
    svc: Arc<dyn playpen_session::SessionService>,
) -> SimpleRunner {
    SimpleRunner::new(
        session.id().to_string(),
        session,
        Box::new(TestProfile),
        Settings::default(),
        svc,
    )
}
