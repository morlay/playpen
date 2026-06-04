use std::path::PathBuf;

use crate::profile::{Profile, SkillInfo};

pub struct Env {
    pub project_root: PathBuf,
    pub profile: Profile,
    pub skills: Vec<SkillInfo>,
    pub agents: String,
}

impl Env {
    pub fn build_system_prompt(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        if !self.profile.system_prompt.is_empty() {
            parts.push(self.profile.system_prompt.clone());
        }

        if !self.agents.is_empty() {
            parts.push(self.agents.clone());
        }

        parts.push(build_env_xml(self));
        if skill_enabled(&self.profile) && !self.skills.is_empty() {
            parts.push(build_skills_xml(&self.skills));
        }

        parts.join("\n\n")
    }
}

fn build_env_xml(env: &Env) -> String {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    format!(
        "<env>\n  <project_root>{}</project_root>\n  <platform>{}</platform>\n  <shell>{}</shell>\n  <date>{}</date>\n</env>",
        quick_xml::escape::escape(env.project_root.to_string_lossy().as_ref()),
        quick_xml::escape::escape(std::env::consts::OS),
        quick_xml::escape::escape(shell.as_str()),
        quick_xml::escape::escape(date.as_str()),
    )
}

fn build_skills_xml(skills: &[SkillInfo]) -> String {
    let mut xml = String::from("<available_skills>\n");
    for skill in skills {
        xml.push_str(&format!(
            "  <skill>\n    <name>{}</name>\n    <description>{}</description>\n    <location>{}</location>\n  </skill>\n",
            quick_xml::escape::escape(skill.name.as_str()),
            quick_xml::escape::escape(skill.description.as_str()),
            quick_xml::escape::escape(skill.location.to_string_lossy().as_ref()),
        ));
    }
    xml.push_str("</available_skills>");
    xml
}

fn skill_enabled(profile: &Profile) -> bool {
    let enabled = profile.skill_enabled.unwrap_or(true);
    let has_read = profile
        .active_tools
        .as_ref()
        .is_none_or(|t| t.contains(&"read".to_string()));
    enabled && has_read
}

#[cfg(test)]
#[path = "env_test.rs"]
mod tests;
