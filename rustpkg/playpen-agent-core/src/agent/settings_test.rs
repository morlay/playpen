use super::*;

#[test]
fn settings_default() {
    let s = Settings::default();
    assert!(s.default_provider.is_none());
    assert!(s.retry.is_none());
}

#[test]
fn settings_apply_toml_overwrite() {
    let mut s = Settings::default();
    let toml: toml::Value = toml::from_str(r#"
        default_provider = "openai"
        default_model = "gpt-4"
    "#).unwrap();
    s.apply_toml(&toml).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("openai"));
    assert_eq!(s.default_model.as_deref(), Some("gpt-4"));
}

#[test]
fn settings_apply_toml_partial() {
    let mut s = Settings::default();
    let toml: toml::Value = toml::from_str(r#"default_provider = "deepseek""#).unwrap();
    s.apply_toml(&toml).unwrap();
    assert_eq!(s.default_provider.as_deref(), Some("deepseek"));
    // default_model 不受影响（None 不覆盖）
    assert!(s.default_model.is_none());
}

#[test]
fn retry_config_default() {
    let r = RetryConfig { max_retries: None, initial_delay_ms: None, backoff_multiplier: None };
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("max_retries"));
}
