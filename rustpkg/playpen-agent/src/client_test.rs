use crate::client::LlmConfig;
use playpen_config::Settings;
use playpen_config::model::{Model, ModelProfile, ModelProvider};
use playpen_profile::AgentProfile;
use std::collections::HashMap;

struct TestProfile;
impl AgentProfile for TestProfile {
    fn name(&self) -> &str {
        "test"
    }
    fn description(&self) -> Option<&str> {
        None
    }
    fn working_dir(&self) -> &std::path::PathBuf {
        static TMP: std::sync::LazyLock<std::path::PathBuf> =
            std::sync::LazyLock::new(|| std::path::PathBuf::from("/tmp"));
        &TMP
    }
    fn model_profile(&self) -> &ModelProfile {
        static MP: ModelProfile = ModelProfile {
            model: String::new(),
            temperature: None,
            top_p: None,
            thinking_level: None,
        };
        &MP
    }
    fn instructions(&self) -> anyhow::Result<String> {
        Ok("test".into())
    }
    fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::Skill>>> {
        Ok(vec![])
    }
    fn tool_enabled(&self, _: &str) -> bool {
        false
    }
    fn with_model_profile(
        &self,
        f: &dyn Fn(&ModelProfile) -> ModelProfile,
    ) -> Box<dyn playpen_profile::AgentProfile> {
        let mp = f(self.model_profile());
        struct P(Box<dyn playpen_profile::AgentProfile>, ModelProfile);
        impl playpen_profile::AgentProfile for P {
            fn name(&self) -> &str {
                self.0.name()
            }
            fn description(&self) -> Option<&str> {
                None
            }
            fn working_dir(&self) -> &std::path::PathBuf {
                self.0.working_dir()
            }
            fn model_profile(&self) -> &ModelProfile {
                &self.1
            }
            fn instructions(&self) -> anyhow::Result<String> {
                self.0.instructions()
            }
            fn available_skills(&self) -> anyhow::Result<Vec<Box<dyn playpen_profile::Skill>>> {
                self.0.available_skills()
            }
            fn tool_enabled(&self, n: &str) -> bool {
                self.0.tool_enabled(n)
            }
            fn with_model_profile(
                &self,
                f: &dyn Fn(&ModelProfile) -> ModelProfile,
            ) -> Box<dyn playpen_profile::AgentProfile> {
                self.0.with_model_profile(f)
            }
        }
        Box::new(P(Box::new(TestProfile), mp))
    }
}

#[test]
fn test_from_settings_with_model_config() {
    let mut providers = HashMap::new();
    providers.insert(
        "openai".into(),
        ModelProvider {
            name: "openai".into(),
            base_url: "https://api.openai.com/".into(),
            api_key: "sk-test".into(),
            models: Some(vec![Model {
                name: "gpt-4o".into(),
                display_name: Some("GPT-4o".into()),
                reasoning_efforts: vec![],
                input_types: vec![],
                context_window: 128000,
                max_tokens: 16384,
                cost: Default::default(),
            }]),
        },
    );
    let settings = Settings {
        default_profile: None,
        sandbox: None,
        model_providers: providers,
    };

    // 用 TestProfile，然后在 with_model_profile 中设置 model
    let profile = TestProfile.with_model_profile(&|mp| ModelProfile {
        model: "openai/gpt-4o".into(),
        ..mp.clone()
    });

    let config = LlmConfig::from_settings(&settings, &*profile).unwrap();
    assert_eq!(config.model, "gpt-4o");
    assert_eq!(config.base_url, "https://api.openai.com");
    assert!(config.model_config.is_some());
    assert_eq!(config.model_config.as_ref().unwrap().max_tokens, 16384);
}

#[test]
fn test_from_settings_missing_provider_error() {
    let settings = Settings {
        default_profile: None,
        sandbox: None,
        model_providers: HashMap::new(),
    };
    let profile = TestProfile.with_model_profile(&|mp| ModelProfile {
        model: "unknown/model".into(),
        ..mp.clone()
    });
    let result = LlmConfig::from_settings(&settings, &*profile);
    assert!(result.is_err(), "不存在的 provider 应返回错误");
}

#[test]
fn test_llm_config_deepseek() {
    let config = LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test".into(),
        model: "deepseek/deepseek-v4-flash".into(),
        model_config: None,
    };
    assert!(config.is_deepseek());
    assert_eq!(config.model, "deepseek/deepseek-v4-flash");
}

#[test]
fn test_llm_config_non_deepseek() {
    let config = LlmConfig {
        base_url: "https://api.openai.com".into(),
        api_key: "sk-test".into(),
        model: "openai/gpt-4".into(),
        model_config: None,
    };
    assert!(!config.is_deepseek());
    assert_eq!(config.model, "openai/gpt-4");
}

#[test]
fn test_llm_config_no_provider_prefix() {
    let config = LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test".into(),
        model: "gpt-4".into(),
        model_config: None,
    };
    // 无 provider 前缀，不视为 deepseek（默认用 openai client）
    assert!(!config.is_deepseek());
    assert_eq!(config.model, "gpt-4");
}
