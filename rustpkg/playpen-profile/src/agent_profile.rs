use std::path::PathBuf;

use playpen_config::model::ModelProfile;

use crate::skill::Skill;

pub trait AgentProfile: Send + Sync {
    fn with_model_profile(
        &self,
        reducer: &dyn Fn(&ModelProfile) -> ModelProfile,
    ) -> Box<dyn AgentProfile>;

    fn name(&self) -> &str;

    fn description(&self) -> Option<&str>;

    fn working_dir(&self) -> &PathBuf;

    fn model_profile(&self) -> &ModelProfile;

    fn instructions(&self) -> anyhow::Result<String>;

    fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn Skill>>>;

    fn tool_enabled(&self, name: &str) -> bool;
}
