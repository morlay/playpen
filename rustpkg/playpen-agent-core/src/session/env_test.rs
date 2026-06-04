use super::*;

#[test]
fn system_prompt_includes_profile_and_env() {
    let env = Env {
        project_root: "/tmp/project".into(),
        profile: Profile {
            name: "default".into(), description: None,
            active_tools: None, skill_enabled: None,
            system_prompt: "你是助手".into(),
        },
        skills: vec![],
        agents: String::new(),
    };
    let prompt = env.build_system_prompt();
    assert!(prompt.contains("你是助手"));
    assert!(prompt.contains("<env>"));
    assert!(prompt.contains("<project_root>/tmp/project</project_root>"));
}

#[test]
fn system_prompt_includes_agents() {
    let env = Env {
        project_root: "/tmp/project".into(),
        profile: Profile {
            name: "default".into(), description: None,
            active_tools: None, skill_enabled: None,
            system_prompt: String::new(),
        },
        skills: vec![],
        agents: "不要使用 find 命令".into(),
    };
    let prompt = env.build_system_prompt();
    assert!(prompt.contains("不要使用 find 命令"));
}
