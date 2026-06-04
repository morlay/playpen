use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_enabled: Option<bool>,
    #[serde(default)]
    pub system_prompt: String,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub location: PathBuf,
    pub source: SkillSource,
}

#[derive(Debug, Clone)]
pub enum SkillSource {
    Global,
    Project,
}

#[derive(Debug, Clone)]
pub struct PromptEnv {
    pub project_root: PathBuf,
    pub working_dir: PathBuf,
    pub skills: Vec<SkillInfo>,
    pub global_agents_content: String,
    pub project_agents_content: String,
}

#[cfg(test)]
#[path = "profile_test.rs"]
mod tests;
