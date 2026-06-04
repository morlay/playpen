use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use super::profile::{Profile, SkillInfo, SkillSource};

pub struct Loader {
    config_dir: PathBuf,
    agents_dir: PathBuf,
    cwd: PathBuf,
}

impl Loader {
    pub fn cwd(&self) -> PathBuf { self.cwd.clone() }

    pub fn new(cwd: PathBuf) -> Self {
        let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/tmp"));
        Self {
            config_dir: home.join(".config").join("playpen"),
            agents_dir: home.join(".agents"),
            cwd,
        }
    }

    pub fn list_profiles(&self) -> anyhow::Result<Vec<Profile>> {
        let dir = self.config_dir.join("agent");
        if !dir.is_dir() { return Ok(vec![]); }

        let mut profiles = Vec::new();
        let mut entries: Vec<_> = fs::read_dir(&dir)
            .with_context(|| format!("读取 agent 目录失败: {}", dir.display()))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let path = entry.path();
            let content = fs::read_to_string(&path)
                .with_context(|| format!("读取文件失败: {}", path.display()))?;
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
            match parse_md_as_profile(name, &content) {
                Ok(p) => profiles.push(p),
                Err(e) => eprintln!("警告: 解析 {} 失败: {:#}", path.display(), e),
            }
        }
        Ok(profiles)
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        let mut map = std::collections::HashMap::new();
        self.scan_skills(&self.agents_dir.join("skills"), SkillSource::Global, &mut map);
        self.scan_skills(&self.cwd.join(".agents").join("skills"), SkillSource::Project, &mut map);
        let mut skills: Vec<_> = map.into_values().collect();
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        skills
    }

    pub fn load_agents(&self) -> String {
        let mut parts = Vec::new();
        let global = self.agents_dir.join("AGENTS.md");
        if global.is_file()
            && let Ok(c) = fs::read_to_string(&global) { parts.push(c); }
        let project = self.cwd.join("AGENTS.md");
        if project.is_file() && project != global
            && let Ok(c) = fs::read_to_string(&project) { parts.push(c); }
        parts.join("\n\n")
    }

    fn scan_skills(&self, dir: &Path, source: SkillSource, map: &mut std::collections::HashMap<String, SkillInfo>) {
        if !dir.is_dir() { return; }
        let entries = match fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
        for entry in entries.flatten() {
            let skill_path = entry.path().join("SKILL.md");
            if !skill_path.is_file() { continue; }
            let name = entry.file_name().to_string_lossy().to_string();
            let description = fs::read_to_string(&skill_path).ok().and_then(|c| {
                c.lines().find(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
                    .map(|l| l.trim().to_string())
            }).unwrap_or_default();
            map.insert(name.clone(), SkillInfo { name, description, location: skill_path, source: source.clone() });
        }
    }
}

fn parse_md_as_profile(name: &str, content: &str) -> anyhow::Result<Profile> {
    let content = content.trim_start();
    let (frontmatter, body) = if let Some(after) = content.strip_prefix("---") {
        if let Some(end) = after.find("\n---") {
            (after[..end].trim(), after[end + 4..].trim_start().to_string())
        } else { ("", content.to_string()) }
    } else { ("", content.to_string()) };

    if frontmatter.is_empty() {
        Ok(Profile { name: name.into(), description: None, active_tools: None, skill_enabled: None, system_prompt: body })
    } else {
        let mut p: Profile = serde_yaml::from_str(frontmatter)
            .map_err(|e| anyhow::anyhow!("解析 YAML 失败: {}", e))?;
        if p.system_prompt.is_empty() { p.system_prompt = body; }
        Ok(p)
    }
}
