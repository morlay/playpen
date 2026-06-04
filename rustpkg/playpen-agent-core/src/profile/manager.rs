use crate::profile::{Loader, Profile, SkillInfo};
use crate::session::env::Env;

pub struct ProfileManager {
    loader: Loader,
}

impl ProfileManager {
    pub fn new(loader: Loader) -> Self {
        Self { loader }
    }

    pub fn list_profiles(&self) -> anyhow::Result<Vec<Profile>> {
        self.loader.list_profiles()
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        self.loader.list_skills()
    }

    /// 根据 profile 名称构建 Env（用于 Session 系统提示词构建）
    pub fn build_env(&self, profile_name: &str) -> anyhow::Result<Env> {
        let profiles = self.list_profiles()?;
        let profile = profiles
            .iter()
            .find(|p| p.name == profile_name)
            .cloned()
            .unwrap_or_else(|| Profile {
                name: profile_name.into(), description: None,
                active_tools: None, skill_enabled: None,
                system_prompt: String::new(),
            });

        Ok(Env {
            project_root: self.loader.cwd(),
            profile,
            skills: self.list_skills(),
            agents: self.loader.load_agents(),
        })
    }
}
