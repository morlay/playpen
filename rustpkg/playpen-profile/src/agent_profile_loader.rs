use std::collections::HashMap;
use std::path::PathBuf;
use std::{fs, path::Path};

use anyhow::Context;
use playpen_config::Dirs;
use playpen_config::model::ModelProfile;
use serde::Deserialize;

use crate::AgentProfile;
use crate::skill::{LocalSkill, Skill, Source as SkillSource};

// ── AgentProfileLoader trait ──────────────────────────────────────────────

pub trait AgentProfileLoader: Send + Sync {
    fn agent_profiles(&self, dirs: &Dirs) -> anyhow::Result<Vec<Box<dyn AgentProfile>>>;
}

// ── LocalAgentProfileLoader ───────────────────────────────────────────────

pub struct LocalAgentProfileLoader;

impl AgentProfileLoader for LocalAgentProfileLoader {
    fn agent_profiles(&self, dirs: &Dirs) -> anyhow::Result<Vec<Box<dyn AgentProfile>>> {
        let profiles_dir = dirs.config_data_dir.join("profiles");
        if !profiles_dir.is_dir() {
            return Ok(vec![]);
        }
        let mut profiles: Vec<Box<dyn AgentProfile>> = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(&profiles_dir)
            .context("读取 profiles 目录失败")?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let dir = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let toml_path = dir.join("profile.toml");
            let Ok(toml_content) = fs::read_to_string(&toml_path) else {
                continue;
            };
            let Ok(mut cfg) = toml::from_str::<ProfileConfig>(&toml_content) else {
                continue;
            };

            let instructions_md =
                fs::read_to_string(dir.join("instructions.md")).unwrap_or_default();

            if !instructions_md.is_empty() {
                cfg.instruction = instructions_md;
            }

            profiles.push(Box::new(LocalAgentProfile::new(name, cfg, dirs.clone())));
        }
        Ok(profiles)
    }
}

// ── ProfileConfig ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProfileConfig {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tools: HashMap<String, bool>,
    #[serde(default)]
    pub instruction: String,
    #[serde(default)]
    pub default_model_profile: ModelProfile,
}

// ── LocalAgentProfile ─────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct LocalAgentProfile {
    name: String,
    dirs: Dirs,
    cfg: ProfileConfig,
}

impl LocalAgentProfile {
    pub(crate) fn new(name: String, cfg: ProfileConfig, dirs: Dirs) -> Self {
        Self { name, cfg, dirs }
    }

    fn scan_skills(
        &self,
        dir: &Path,
        source: SkillSource,
        map: &mut HashMap<String, Box<dyn Skill>>,
    ) {
        if !dir.is_dir() {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let skill_path = entry.path().join("SKILL.md");
            if !skill_path.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 同名 skill 后插入覆盖前（global 先扫描，project 后扫描）
            if let Some(skill) = LocalSkill::load(skill_path, source) {
                map.insert(name, Box::new(skill) as Box<dyn Skill>);
            }
        }
    }

    fn load_skills(&self) -> Vec<Box<dyn Skill>> {
        let mut map: HashMap<String, Box<dyn Skill>> = HashMap::new();

        // global: ~/.agents/skills/<name>/SKILL.md
        self.scan_skills(
            &self.dirs.agents_dir.join("skills"),
            SkillSource::Global,
            &mut map,
        );

        // project: <working_dir>/.agents/skills/<name>/SKILL.md (覆盖 global)
        self.scan_skills(
            &self.dirs.working_dir.join(".agents").join("skills"),
            SkillSource::Project,
            &mut map,
        );

        let mut skills: Vec<_> = map.into_values().collect();
        skills.sort_by(|a, b| a.metadata().name.cmp(&b.metadata().name));
        skills
    }
}

impl AgentProfile for LocalAgentProfile {
    fn name(&self) -> &str {
        &self.name
    }

    fn working_dir(&self) -> &PathBuf {
        &self.dirs.working_dir
    }

    fn description(&self) -> Option<&str> {
        self.cfg.description.as_deref()
    }

    fn model_profile(&self) -> &ModelProfile {
        &self.cfg.default_model_profile
    }

    fn with_model_profile(
        &self,
        reducer: &dyn Fn(&ModelProfile) -> ModelProfile,
    ) -> Box<dyn AgentProfile> {
        let mut cfg = self.cfg.clone();
        cfg.default_model_profile = reducer(&self.cfg.default_model_profile);
        Box::new(Self {
            cfg,
            ..self.clone()
        })
    }

    fn instructions(&self) -> anyhow::Result<String> {
        let mut parts: Vec<String> = Vec::new();
        if !self.cfg.instruction.is_empty() {
            parts.push(self.cfg.instruction.clone());
        }

        if let Ok(project_agents_md) = fs::read_to_string(self.dirs.working_dir.join("AGENTS.md"))
            && !project_agents_md.is_empty()
        {
            parts.push(project_agents_md);
        }

        parts.push(build_env_xml(&self.dirs.working_dir));

        let skills = self.available_skills()?;
        let available_skills = inject_available_skills(&skills);
        if !available_skills.is_empty() {
            parts.push(available_skills);
        }
        Ok(parts.join("\n\n"))
    }

    fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn Skill>>> {
        Ok(self.load_skills())
    }

    fn tool_enabled(&self, name: &str) -> bool {
        self.cfg.tools.get(name).copied().unwrap_or(true)
    }
}

// ── XML helpers ───────────────────────────────────────────────────────────

fn build_env_xml(cwd: &std::path::Path) -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let root = htmlescape::encode_minimal(cwd.to_string_lossy().as_ref());
    let platform = htmlescape::encode_minimal(std::env::consts::OS);
    let sh = htmlescape::encode_minimal(shell.as_str());
    let d = htmlescape::encode_minimal(date.as_str());
    format!(
        r#"<env>
  <project_root>{root}</project_root>
  <platform>{platform}</platform>
  <shell>{sh}</shell>
  <date>{d}</date>
</env>"#
    )
}

fn inject_available_skills(skills: &[Box<dyn Skill>]) -> String {
    let visible: Vec<_> = skills
        .iter()
        .filter(|s| !s.metadata().disable_model_invocation.unwrap_or(false))
        .collect();

    if visible.is_empty() {
        return String::new();
    }

    let mut xml = String::from(
        r#"
可用技能: 你可以通过 read 工具读取对应 SKILL.md 路径进行调阅

<available_skills>
"#,
    );
    for skill in &visible {
        let meta = skill.metadata();
        let name = htmlescape::encode_minimal(meta.name.as_str());
        let desc = htmlescape::encode_minimal(meta.description.as_str());
        let loc = htmlescape::encode_minimal(skill.location().to_string_lossy().as_ref());
        xml.push_str(&format!(
            r#"  <skill>
    <name>{name}</name>
    <description>{desc}</description>
    <location>{loc}</location>
  </skill>
"#
        ));
    }
    xml.push_str("</available_skills>");
    xml
}

#[cfg(test)]
#[path = "agent_profile_loader_test.rs"]
mod tests;
