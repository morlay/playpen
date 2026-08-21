use crate::client::LlmConfig;
use crate::testing::TestProfile;
use playpen_config::Settings;
use playpen_config::model::{Model, ModelProfile, ModelProvider};
use playpen_profile::AgentProfile;
use std::collections::HashMap;

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
    let profile = TestProfile::default().with_model_profile(&|mp| ModelProfile {
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
    let profile = TestProfile::default().with_model_profile(&|mp| ModelProfile {
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
    assert!(config.is_deepseek_compat());
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
    assert!(!config.is_deepseek_compat());
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
    assert!(!config.is_deepseek_compat());
    assert_eq!(config.model, "gpt-4");
}

#[test]
fn test_llm_config_glm_and_mimo() {
    let config_glm = LlmConfig {
        base_url: "https://open.bigmodel.cn".into(),
        api_key: "sk-test".into(),
        model: "glm-4".into(),
        model_config: None,
    };
    assert!(
        config_glm.is_deepseek_compat(),
        "glm 应使用 DeepSeek 兼容流式"
    );

    let config_mimo = LlmConfig {
        base_url: "https://api.mimo.com".into(),
        api_key: "sk-test".into(),
        model: "mimo-pro".into(),
        model_config: None,
    };
    assert!(
        config_mimo.is_deepseek_compat(),
        "mimo 应使用 DeepSeek 兼容流式"
    );

    // 普通模型不匹配
    let config_normal = LlmConfig {
        base_url: "https://api.other.com".into(),
        api_key: "sk-test".into(),
        model: "qwen-2.5".into(),
        model_config: None,
    };
    assert!(!config_normal.is_deepseek_compat(), "其他模型不匹配");
}

// ── file 块改写（DeepSeek Files API 格式） ──

#[test]
fn test_rewrite_file_blocks_to_deepseek_format() {
    let mut body = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "file", "file": {"file_id": "file-api-abc123"}}
            ]
        }]
    });
    crate::client::rewrite_file_blocks(&mut body);

    let content = &body["messages"][0]["content"];
    assert_eq!(content[1]["type"], "file");
    assert_eq!(content[1]["file_id"], "file-api-abc123");
    assert!(
        content[1].get("file").is_none(),
        "OpenAI 嵌套 file 对象应被移除"
    );
    // 文本块不受影响
    assert_eq!(content[0]["text"], "what is this?");
}

#[test]
fn test_rewrite_file_blocks_keeps_file_data() {
    let mut body = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "file", "file": {"file_data": "data:image/png;base64,AAAA", "filename": "a.png"}}
            ]
        }]
    });
    crate::client::rewrite_file_blocks(&mut body);

    let part = &body["messages"][0]["content"][0];
    assert_eq!(part["file_data"], "data:image/png;base64,AAAA");
    assert_eq!(part["filename"], "a.png");
}

#[test]
fn test_finalize_flattens_text_only_content() {
    let mut body = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "hello"},
                {"type": "text", "text": "world"}
            ]
        }]
    });
    crate::client::deepseek_finalize_request_body(&mut body).unwrap();
    assert_eq!(body["messages"][0]["content"], "hello\nworld");
}

#[test]
fn test_finalize_keeps_mixed_content_array() {
    let mut body = serde_json::json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "what is this?"},
                {"type": "file", "file": {"file_id": "file-api-x"}}
            ]
        }]
    });
    crate::client::deepseek_finalize_request_body(&mut body).unwrap();
    // 含 file 块的数组保留为数组（不扁平化）
    let content = &body["messages"][0]["content"];
    assert!(content.is_array(), "混合内容数组应保留");
    assert_eq!(content.as_array().unwrap().len(), 2);
}

#[test]
fn test_finalize_tool_calls_add_index() {
    let mut body = serde_json::json!({
        "messages": [{
            "role": "assistant",
            "content": "",
            "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "read", "arguments": "{}"}}]
        }]
    });
    crate::client::deepseek_finalize_request_body(&mut body).unwrap();
    assert_eq!(body["messages"][0]["tool_calls"][0]["index"], 0);
}

#[test]
fn test_supports_image_by_config() {
    let config = LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test".into(),
        model: "deepseek/deepseek-v4-flash".into(),
        model_config: Some(Model {
            name: "deepseek-v4-flash".into(),
            display_name: None,
            reasoning_efforts: vec![],
            input_types: vec![playpen_config::model::InputType::Image],
            context_window: 128000,
            max_tokens: 16384,
            cost: Default::default(),
        }),
    };
    assert!(config.supports_image(), "配置声明 Image 时应支持");
}

#[test]
fn test_supports_image_falls_back_to_name() {
    let config = LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test".into(),
        model: "deepseek/deepseek-v4-flash-vision-exp".into(),
        model_config: None,
    };
    assert!(config.supports_image(), "无配置时按模型名兜底");
}

#[test]
fn test_supports_image_false_for_text_model() {
    let config = LlmConfig {
        base_url: "https://api.deepseek.com".into(),
        api_key: "sk-test".into(),
        model: "deepseek/deepseek-v4-flash".into(),
        model_config: None,
    };
    assert!(!config.supports_image(), "文本模型不支持图片");
}
