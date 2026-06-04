use super::*;

#[test]
fn profile_serde() {
    let p = Profile {
        name: "code".into(),
        description: Some("coding agent".into()),
        active_tools: Some(vec!["read".into(), "bash".into()]),
        skill_enabled: Some(true),
        system_prompt: "你是一个编程助手".into(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: Profile = serde_json::from_str(&json).unwrap();
    assert_eq!(back.name, "code");
    assert_eq!(back.system_prompt, "你是一个编程助手");
    assert_eq!(back.active_tools.unwrap().len(), 2);
}

#[test]
fn profile_serde_minimal() {
    let p = Profile {
        name: "default".into(),
        description: None,
        active_tools: None,
        skill_enabled: None,
        system_prompt: String::new(),
    };
    let json = serde_json::to_string(&p).unwrap();
    // 空字段被 skip
    assert!(!json.contains("description"));
    assert!(!json.contains("active_tools"));
}
