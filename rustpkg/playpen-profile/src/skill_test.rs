use std::path::PathBuf;

use super::{LocalSkill, Metadata, Skill, Source};

// ── parse_skill_md ────────────────────────────────────────────────────

#[test]
fn parse_valid_frontmatter() {
    let raw = r#"---
name: my-skill
description: A test skill
---

# Instructions

do something
"#;
    let (meta, body) = super::parse_skill(raw).unwrap();
    assert_eq!(meta.name, "my-skill");
    assert_eq!(meta.description, "A test skill");
    assert!(meta.disable_model_invocation.is_none());
    assert_eq!(body, "# Instructions\n\ndo something");
}

#[test]
fn parse_missing_frontmatter_returns_none() {
    let raw = "no frontmatter here";
    assert!(super::parse_skill(raw).is_none());
}

#[test]
fn parse_invalid_yaml_returns_none() {
    let raw = "---\nname: [invalid\n---\n\nbody";
    assert!(super::parse_skill(raw).is_none());
}

#[test]
fn parse_frontmatter_with_disabled_invocation() {
    let raw = r#"---
name: read-only
description: Read only skill
disable-model-invocation: true
---

content
"#;
    let (meta, body) = super::parse_skill(raw).unwrap();
    assert_eq!(meta.name, "read-only");
    assert_eq!(meta.disable_model_invocation, Some(true));
    assert_eq!(body, "content");
}

#[test]
fn parse_empty_body() {
    let raw = r#"---
name: empty
description: Empty body
---
"#;
    let (meta, body) = super::parse_skill(raw).unwrap();
    assert_eq!(meta.name, "empty");
    assert_eq!(body, "");
}

#[test]
fn parse_frontmatter_with_license_and_metadata() {
    let raw = r#"---
name: licensed-skill
description: A skill with license
license: MIT
metadata:
  key: value
  count: 42
---

body
"#;
    let (meta, body) = super::parse_skill(raw).unwrap();
    assert_eq!(meta.name, "licensed-skill");
    assert_eq!(meta.license.as_deref(), Some("MIT"));
    let meta_map = meta.metadata.as_ref().unwrap();
    assert_eq!(meta_map["key"], serde_json::Value::String("value".into()));
    assert_eq!(meta_map["count"], serde_json::Value::Number(42.into()));
    assert_eq!(body, "body");
}

// ── LocalSkill ────────────────────────────────────────────────────────

#[test]
fn local_skill_new_and_accessors() {
    let meta = Metadata {
        name: "test".into(),
        description: "Test skill".into(),
        license: None,
        metadata: None,
        disable_model_invocation: None,
    };
    let skill = LocalSkill::new(
        meta.clone(),
        PathBuf::from("/tmp/skills/test/SKILL.md"),
        Source::Global,
        "instructions here".into(),
    );

    assert_eq!(skill.metadata().name, "test");
    assert_eq!(skill.metadata().description, "Test skill");
    assert_eq!(
        skill.location(),
        &PathBuf::from("/tmp/skills/test/SKILL.md")
    );
    assert_eq!(skill.source(), Source::Global);
    assert_eq!(skill.instructions(), "instructions here");
}

#[test]
fn local_skill_load_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir.path().join(".agents/skills/my-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: my-skill
description: A test skill
---

# Instructions

do something
"#,
    )
    .unwrap();

    let skill = LocalSkill::load(skill_dir.join("SKILL.md"), Source::Project).unwrap();
    assert_eq!(skill.metadata().name, "my-skill");
    assert_eq!(skill.metadata().description, "A test skill");
    assert_eq!(skill.source(), Source::Project);
    assert_eq!(skill.instructions(), "# Instructions\n\ndo something");
}

#[test]
fn local_skill_load_missing_file_returns_none() {
    let skill = LocalSkill::load(PathBuf::from("/tmp/nonexistent/SKILL.md"), Source::Global);
    assert!(skill.is_none());
}

#[test]
fn local_skill_load_invalid_content_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("SKILL.md");
    std::fs::write(&path, "no frontmatter").unwrap();

    let skill = LocalSkill::load(path, Source::Global);
    assert!(skill.is_none());
}
