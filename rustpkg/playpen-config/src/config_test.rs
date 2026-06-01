use super::*;

#[test]
fn merge_settings_overrides() {
    let a: toml::Value = toml::from_str(r#"default_profile = "code""#).unwrap();
    let b: toml::Value = toml::from_str(
        r#"
[model_providers.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
api_wire = "chat"
api_key = "sk-test"
"#,
    )
    .unwrap();
    let s = merge_settings(&[a, b]).unwrap();
    assert_eq!(s.default_profile.as_deref(), Some("code"));
    // 预设 deepseek 被用户配置覆盖，长度仍为 1
    assert_eq!(s.model_providers.len(), 1);
}

#[test]
fn merge_settings_last_wins() {
    let a: toml::Value = toml::from_str(r#"default_profile = "first""#).unwrap();
    let b: toml::Value = toml::from_str(r#"default_profile = "second""#).unwrap();
    let s = merge_settings(&[a, b]).unwrap();
    assert_eq!(s.default_profile.as_deref(), Some("second"));
}

#[test]
fn merge_sandbox_parses_section() {
    let v: toml::Value = toml::from_str(
        r#"
[sandbox.shell]
allow_pipe = false
"#,
    )
    .unwrap();
    let s = merge_sandbox(&[v]).unwrap();
    assert_eq!(s.shell.as_ref().unwrap().allow_pipe, Some(false));
}

#[test]
fn appconfig_load_or_default_works() {
    let dir = tempfile::tempdir().unwrap();
    let config = AppConfig::load_or_default(dir.path());
    // 预设 provider 已注入
    assert!(config.settings.model_providers.contains_key("deepseek"));
    assert_eq!(config.settings.model_providers.len(), 1);
}

#[test]
fn merge_sandbox_profile_parses() {
    let v: toml::Value = toml::from_str(
        r#"
[sandbox]
enabled = true

[sandbox.filesystem]
access = ["rw /tmp", "r- /usr"]

[sandbox.shell]
allow_pipe = false
"#,
    )
    .unwrap();
    let s = merge_sandbox(&[v]).unwrap();
    assert_eq!(s.shell.as_ref().unwrap().allow_pipe, Some(false));
}
