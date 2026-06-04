use super::*;

fn toml_val(s: &str) -> toml::Value {
    toml::from_str(s).unwrap()
}

// ── Settings 合并 ──

#[test]
fn merge_settings_single_file() {
    let v = toml_val(r#"default_provider = "openai""#);
    let s = merge_settings(&[v]).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("openai"));
    assert!(s.default_model.is_none());
}

#[test]
fn merge_settings_multi_file_overwrite() {
    let v1 = toml_val(r#"default_provider = "openai""#);
    let v2 = toml_val(r#"default_provider = "deepseek""#);
    let s = merge_settings(&[v1, v2]).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("deepseek"));
}

#[test]
fn merge_settings_partial_override() {
    let v1 = toml_val(
        "default_provider = \"openai\"\ndefault_model = \"gpt-4\"\n"
    );
    let v2 = toml_val("default_model = \"gpt-4o\"\n");
    let s = merge_settings(&[v1, v2]).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("openai"));
    assert_eq!(s.default_model.as_deref(), Some("gpt-4o"));
}

#[test]
fn merge_settings_empty_files() {
    let s = merge_settings(&[]).unwrap();
    assert!(s.default_provider.is_none());
}

#[test]
fn merge_settings_retry_config() {
    let v = toml_val("[retry]\nmax_retries = 5\ninitial_delay_ms = 2000\n");
    let s = merge_settings(&[v]).unwrap();
    let r = s.retry.unwrap();
    assert_eq!(r.max_retries, Some(5));
    assert_eq!(r.initial_delay_ms, Some(2000));
}

#[test]
fn merge_settings_layered() {
    let global = toml_val("default_provider = \"openai\"\n");
    let conf_d = toml_val("default_model = \"gpt-4o\"\n");
    let project = toml_val("default_provider = \"deepseek\"\n");

    // debug: 检查每步中间状态
    let s1 = merge_settings(std::slice::from_ref(&global)).unwrap();
    assert_eq!(s1.default_provider.as_deref(), Some("openai"));

    let s2 = merge_settings(&[global.clone(), conf_d.clone()]).unwrap();
    assert_eq!(s2.default_provider.as_deref(), Some("openai"));
    assert_eq!(s2.default_model.as_deref(), Some("gpt-4o"));

    let s = merge_settings(&[global, conf_d, project]).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("deepseek"));
    assert_eq!(s.default_model.as_deref(), Some("gpt-4o"));
}

// ── Providers 合并 ──

#[test]
fn merge_providers_single() {
    let v = toml_val(
        "[providers.openai]\nid = \"openai\"\nname = \"OpenAI\"\nbase_url = \"https://api.openai.com/v1\"\napi = \"openai-completions\"\napi_key = \"sk-test\"\n"
    );
    let ps = merge_providers(&[v]).unwrap();
    assert_eq!(ps.len(), 1);
    assert_eq!(ps.get("openai").unwrap().base_url, "https://api.openai.com/v1");
}

#[test]
fn merge_providers_override_same_id() {
    let v1 = toml_val(
        "[providers.deepseek]\nid = \"deepseek\"\nname = \"DS\"\nbase_url = \"https://old.example.com/v1\"\napi = \"openai-completions\"\napi_key = \"old-key\"\n"
    );
    let v2 = toml_val(
        "[providers.deepseek]\nid = \"deepseek\"\nname = \"DeepSeek\"\nbase_url = \"https://api.deepseek.com/v1\"\napi = \"openai-completions\"\napi_key = \"new-key\"\n"
    );
    let ps = merge_providers(&[v1, v2]).unwrap();
    assert_eq!(ps.len(), 1);
    assert_eq!(ps.get("deepseek").unwrap().base_url, "https://api.deepseek.com/v1");
}

#[test]
fn merge_providers_different_ids() {
    let v1 = toml_val(
        "[providers.a]\nid = \"a\"\nname = \"A\"\nbase_url = \"http://a/v1\"\napi = \"openai-completions\"\napi_key = \"ka\"\n"
    );
    let v2 = toml_val(
        "[providers.b]\nid = \"b\"\nname = \"B\"\nbase_url = \"http://b/v1\"\napi = \"openai-completions\"\napi_key = \"kb\"\n"
    );
    let ps = merge_providers(&[v1, v2]).unwrap();
    assert_eq!(ps.len(), 2);
}

// ── Sandbox 合并 ──

#[test]
fn merge_sandbox_single() {
    let v = toml_val("[sandbox.network]\naccess = [\"*.example.com\"]\n");
    let s = merge_sandbox(&[v]).unwrap();
    assert!(s.network.is_some());
}

#[test]
fn merge_sandbox_overlay() {
    use merge::Merge;
    let mut c1 = SandboxConfig { filesystem: Some(sandbox::config::AllowSection { access: vec!["rw .".into()] }), ..Default::default() };
    let c2 = SandboxConfig { filesystem: Some(sandbox::config::AllowSection { access: vec!["-- .env".into()] }), ..Default::default() };

    c1.merge(c2);
    let fs = c1.filesystem.unwrap();
    assert_eq!(fs.access.len(), 2, "merge failed: {:?}", fs.access);
}

#[test]
fn merge_sandbox_top_level() {
    // 也支持顶层 [network] 写法
    let v = toml_val("[network]\naccess = [\"*.example.com\"]\n");
    let s = merge_sandbox(&[v]).unwrap();
    assert!(s.network.is_some());
}

#[test]
fn merge_sandbox_empty() {
    let s = merge_sandbox(&[]).unwrap();
    assert!(s.filesystem.is_none());
    assert!(s.network.is_none());
}

// ── AppConfig::load_from ──

#[test]
fn load_from_empty_conf_d() {
    let tmp = tempfile::tempdir().unwrap();
    let conf_d = tmp.path().join("conf.d");
    std::fs::create_dir_all(&conf_d).unwrap();

    let config = AppConfig::load_from(&conf_d, vec![]).unwrap();

    assert!(config.settings.default_provider.is_none());
    assert!(config.providers.is_empty());
    assert!(config.profiles.is_empty());
}

#[test]
fn load_from_single_file() {
    let tmp = tempfile::tempdir().unwrap();
    let conf_d = tmp.path().join("conf.d");
    std::fs::create_dir_all(&conf_d).unwrap();

    let toml_content = "\
default_provider = \"openai\"\n\
default_model = \"gpt-4o\"\n\
\n\
[providers.openai]\n\
id = \"openai\"\n\
name = \"OpenAI\"\n\
base_url = \"https://api.openai.com/v1\"\n\
api = \"openai-completions\"\n\
api_key = \"sk-test\"\n";
    std::fs::write(conf_d.join("01-openai.toml"), toml_content).unwrap();

    let config = AppConfig::load_from(&conf_d, vec![]).unwrap();

    assert_eq!(config.settings.default_provider.as_deref(), Some("openai"));
    assert_eq!(config.settings.default_model.as_deref(), Some("gpt-4o"));
    assert_eq!(config.providers.len(), 1);
    assert!(config.providers.contains_key("openai"));
}

#[test]
fn load_from_ignores_non_toml() {
    let tmp = tempfile::tempdir().unwrap();
    let conf_d = tmp.path().join("conf.d");
    std::fs::create_dir_all(&conf_d).unwrap();

    std::fs::write(conf_d.join("README.md"), "# Config").unwrap();
    std::fs::write(conf_d.join(".DS_Store"), "").unwrap();

    let config = AppConfig::load_from(&conf_d, vec![]).unwrap();

    assert!(config.settings.default_provider.is_none());
}

#[test]
fn load_from_multi_file_override() {
    let tmp = tempfile::tempdir().unwrap();
    let conf_d = tmp.path().join("conf.d");
    std::fs::create_dir_all(&conf_d).unwrap();

    std::fs::write(conf_d.join("01-base.toml"),
        "default_provider = \"openai\"\ndefault_model = \"gpt-4\"\n").unwrap();
    std::fs::write(conf_d.join("02-override.toml"),
        "default_model = \"gpt-4o\"\n").unwrap();

    let config = AppConfig::load_from(&conf_d, vec![]).unwrap();

    assert_eq!(config.settings.default_provider.as_deref(), Some("openai"));
    assert_eq!(config.settings.default_model.as_deref(), Some("gpt-4o"));
}

#[test]
fn load_from_passes_profiles() {
    let tmp = tempfile::tempdir().unwrap();
    let conf_d = tmp.path().join("conf.d");
    std::fs::create_dir_all(&conf_d).unwrap();

    let profiles = vec![Profile {
        name: "default".into(),
        description: Some("默认".into()),
        active_tools: None,
        skill_enabled: None,
        system_prompt: "你是一个助手".into(),
    }];

    let config = AppConfig::load_from(&conf_d, profiles).unwrap();

    assert_eq!(config.profiles.len(), 1);
    assert_eq!(config.profiles[0].name, "default");
}

// ── expand_env_vars ──

#[test]
fn expand_env_vars_basic() {
    unsafe { std::env::set_var("PLAYPEN_TEST_VAR", "hello"); }
    assert_eq!(expand_env_vars("$PLAYPEN_TEST_VAR"), "hello");
    assert_eq!(expand_env_vars("${PLAYPEN_TEST_VAR}"), "hello");
}

#[test]
fn expand_env_vars_missing() {
    assert_eq!(expand_env_vars("$NONEXISTENT_VAR"), "$NONEXISTENT_VAR");
}
