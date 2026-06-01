use std::collections::HashMap;

use playpen_config::Dirs;
use playpen_config::model::ModelProfile;

use super::{AgentProfile, AgentProfileLoader, LocalAgentProfileLoader, ProfileConfig};
use super::{LocalAgentProfile, SkillSource};

// ── helpers ───────────────────────────────────────────────────────────

fn make_dirs(root: &std::path::Path) -> Dirs {
    Dirs {
        working_dir: root.to_path_buf(),
        config_data_dir: root.join(".config/playpen"),
        agents_dir: root.join(".global-agents"),
    }
}

fn write_profile(root: &std::path::Path, name: &str, toml: &str) {
    let dir = root.join(".config/playpen/profiles").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("profile.toml"), toml).unwrap();
}

fn write_instructions(root: &std::path::Path, name: &str, md: &str) {
    let dir = root.join(".config/playpen/profiles").join(name);
    std::fs::write(dir.join("instructions.md"), md).unwrap();
}

fn write_skill(root: &std::path::Path, source_dir: &str, name: &str, md: &str) {
    let dir = root.join(source_dir).join("skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("SKILL.md"), md).unwrap();
}

// ── AgentProfileLoader ────────────────────────────────────────────────

#[test]
fn agent_profiles_no_directory_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert!(profiles.is_empty());
}

#[test]
fn agent_profiles_valid_profile() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(
        dir.path(),
        "default",
        r#"
description = "Default profile"
instruction = "You are helpful"

[default_model_profile]
model = "deepseek/chat"
temperature = 0.7
"#,
    );
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert_eq!(profiles.len(), 1);

    let p = &profiles[0];
    assert_eq!(p.name(), "default");
    assert_eq!(p.description(), Some("Default profile"));
    assert_eq!(p.model_profile().model, "deepseek/chat");
    assert_eq!(p.model_profile().temperature, Some(0.7));
    assert!(p.tool_enabled("bash")); // 默认启用
}

#[test]
fn agent_profiles_missing_toml_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let profiles_dir = dir.path().join(".config/playpen/profiles/empty");
    std::fs::create_dir_all(&profiles_dir).unwrap();
    // 没有 profile.toml
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert!(profiles.is_empty());
}

#[test]
fn agent_profiles_invalid_toml_skipped() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(dir.path(), "broken", "invalid [[toml = {{{");
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert!(profiles.is_empty());
}

#[test]
fn agent_profiles_sorted_by_name() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(dir.path(), "zed", r#"description = "Zed""#);
    write_profile(dir.path(), "alice", r#"description = "Alice""#);
    write_profile(dir.path(), "bob", r#"description = "Bob""#);
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert_eq!(profiles.len(), 3);
    assert_eq!(profiles[0].name(), "alice");
    assert_eq!(profiles[1].name(), "bob");
    assert_eq!(profiles[2].name(), "zed");
}

#[test]
fn agent_profiles_instructions_md_overrides_toml() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(dir.path(), "custom", r#"instruction = "toml instruction""#);
    write_instructions(dir.path(), "custom", "md instruction");
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert_eq!(profiles.len(), 1);
    let p = &profiles[0];
    let instr = p.instructions().unwrap();
    assert!(
        instr.starts_with("md instruction"),
        "预期以 instructions.md 内容开头，实际: {instr}"
    );
}

#[test]
fn agent_profiles_tool_config() {
    let dir = tempfile::tempdir().unwrap();
    write_profile(
        dir.path(),
        "restricted",
        r#"
[tools]
bash = false
read = true
"#,
    );
    let dirs = make_dirs(dir.path());
    let loader = LocalAgentProfileLoader;
    let profiles = loader.agent_profiles(&dirs).unwrap();
    assert_eq!(profiles.len(), 1);
    let p = &profiles[0];
    assert!(!p.tool_enabled("bash"));
    assert!(p.tool_enabled("read"));
    assert!(p.tool_enabled("unknown")); // 默认启用
}

// ── LocalAgentProfile.available_skills ─────────────────────────────────

fn make_skill_md(name: &str, description: &str, body: &str) -> String {
    format!(
        r#"---
name: {name}
description: {description}
---

{body}
"#
    )
}

#[test]
fn available_skills_from_global_dir() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    write_skill(
        dir.path(),
        ".global-agents",
        "code-review",
        &make_skill_md("code-review", "Review code", "review instructions"),
    );

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    let skills = profile.available_skills().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].metadata().name, "code-review");
    assert_eq!(skills[0].source(), SkillSource::Global);
}

#[test]
fn available_skills_from_project_dir() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    write_skill(
        dir.path(),
        ".global-agents",
        "global-skill",
        &make_skill_md("global-skill", "Global", "global"),
    );
    write_skill(
        dir.path(),
        ".agents",
        "project-skill",
        &make_skill_md("project-skill", "Project", "project"),
    );

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    let skills = profile.available_skills().unwrap();
    assert_eq!(skills.len(), 2);

    let names: Vec<&str> = skills.iter().map(|s| s.metadata().name.as_str()).collect();
    assert!(names.contains(&"global-skill"));
    assert!(names.contains(&"project-skill"));

    // project skill 的 source 为 Project
    let ps = skills
        .iter()
        .find(|s| s.metadata().name == "project-skill")
        .unwrap();
    assert_eq!(ps.source(), SkillSource::Project);
}

#[test]
fn available_skills_project_overrides_global() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    // global 同名 skill
    write_skill(
        dir.path(),
        ".global-agents",
        "common",
        &make_skill_md("common", "global version", "global content"),
    );

    // project 同名 skill — 应覆盖 global
    write_skill(
        dir.path(),
        ".agents",
        "common",
        &make_skill_md("common", "project version", "project content"),
    );

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    let skills = profile.available_skills().unwrap();
    assert_eq!(skills.len(), 1, "同名 skill 应合并为一条");
    assert_eq!(
        skills[0].source(),
        SkillSource::Project,
        "project 应覆盖 global"
    );
    assert_eq!(skills[0].instructions(), "project content");
}

#[test]
fn available_skills_no_directories_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());
    // 不创建任何 skill 目录

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    let skills = profile.available_skills().unwrap();
    assert!(skills.is_empty());
}

#[test]
fn available_skills_skips_invalid_skill_files() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    // 有效的 skill
    write_skill(
        dir.path(),
        ".global-agents",
        "valid",
        &make_skill_md("valid", "Valid", "ok"),
    );

    // 无 frontmatter 的 SKILL.md（无效）
    let bad_dir = dir.path().join(".agents/skills/bad");
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::write(bad_dir.join("SKILL.md"), "no frontmatter").unwrap();

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    let skills = profile.available_skills().unwrap();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].metadata().name, "valid");
}

// ── LocalAgentProfile.with_model_profile ───────────────────────────────

#[test]
fn with_model_profile_reduces_model() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: Some("original".into()),
            tools: HashMap::new(),
            instruction: "hello".into(),
            default_model_profile: ModelProfile {
                model: "deepseek/chat".into(),
                ..Default::default()
            },
        },
        dirs,
    );

    let new_profile = profile.with_model_profile(&|mp| ModelProfile {
        model: "openai/gpt-4".into(),
        ..mp.clone()
    });

    assert_eq!(new_profile.name(), "test");
    assert_eq!(new_profile.description(), Some("original"));
    assert_eq!(new_profile.model_profile().model, "openai/gpt-4");
    // 原 profile 不受影响
    assert_eq!(profile.model_profile().model, "deepseek/chat");
}

// ── LocalAgentProfile.tool_enabled ─────────────────────────────────────

#[test]
fn tool_enabled_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = make_dirs(dir.path());

    let profile = LocalAgentProfile::new(
        "test".into(),
        ProfileConfig {
            description: None,
            tools: HashMap::new(),
            instruction: String::new(),
            default_model_profile: ModelProfile::default(),
        },
        dirs,
    );
    assert!(profile.tool_enabled("anything"));
}
