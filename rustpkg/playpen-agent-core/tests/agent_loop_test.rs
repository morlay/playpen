//! 端到端 Agent 闭环测试
use std::sync::Arc;

use playpen_agent_core::agent::service::AgentSessionService;
use playpen_agent_core::model::{Model, Provider};
use playpen_agent_core::model::Registry;
use playpen_agent_core::profile::Loader;
use playpen_agent_core::profile::manager::ProfileManager;
use playpen_agent_core::session::store::SessionManager;
use playpen_agent_core::workspace::Workspace;
use sandbox::config::{ParsedRule, RulePrefix};

fn test_ws(tmp: &std::path::Path) -> Arc<Workspace> {
    let rules = vec![ParsedRule {
        raw: "rw .".into(),
        prefix: RulePrefix::Allow,
        pattern: tmp.to_string_lossy().to_string(),
    }];
    Arc::new(Workspace::new(
        tmp.to_path_buf(),
        Arc::new(sandbox::SandboxConfig {
            shell_bin: "/bin/zsh".into(),
            policy_class: Default::default(),
            exec_policy: Default::default(),
            allow_pipe: false,
            allow_multiple: false,
        }),
        rules,
    ))
}

#[test]
fn agent_session_new() {
    let home = tempfile::tempdir().unwrap();

    let agent_dir = home.path().join(".config/playpen/agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    std::fs::write(agent_dir.join("default.md"), "---\n---\n你是一个测试助手").unwrap();

    let agents_dir = home.path().join(".agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(agents_dir.join("AGENTS.md"), "使用中文").unwrap();

    unsafe { std::env::set_var("HOME", home.path().to_string_lossy().as_ref()); }

    let cwd = home.path().join("project");
    std::fs::create_dir_all(&cwd).unwrap();
    let ws = test_ws(&cwd);

    let loader = Loader::new(cwd.clone());
    let profile_manager = Arc::new(ProfileManager::new(loader));
    let session_manager = Arc::new(SessionManager::new());

    let mut providers = std::collections::HashMap::new();
    providers.insert("test".into(), Provider {
        id: "test".into(), name: "test".into(),
        api: "openai-completions".into(), base_url: "http://localhost:1".into(),
        api_key: "sk-test".into(),
        models: Some(vec![Model {
            id: "test-model".into(), name: "Test".into(),
            reasoning_efforts: vec!["off".into()], input: vec!["text".into()],
            context_window: 128000, max_tokens: 4096,
            cost: Default::default(),
        }]),
    });
    let registry = Registry::new(providers);
    let client = registry.build_client("test").unwrap();

    let service = AgentSessionService::new(
        session_manager.clone(),
        profile_manager,
        client,
        ws,
        None,
    );

    let model = Model {
        id: "test-model".into(), name: "Test".into(),
        reasoning_efforts: vec!["off".into()], input: vec!["text".into()],
        context_window: 128000, max_tokens: 4096,
        cost: Default::default(),
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let session = rt.block_on(service.new_session(model)).unwrap();

    assert!(!session.session_id.is_empty());
    let stored = session_manager.get(&session.session_id).unwrap();
    assert_eq!(stored.title, "new session");
    assert!(stored.system_prompt.contains("你是一个测试助手"));
    assert!(stored.system_prompt.contains("使用中文"));
    assert!(stored.system_prompt.contains("<env>"));
}
