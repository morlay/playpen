use crate::model::{Cost, Model, Provider};

/// 内置 DeepSeek 模型配置
pub fn builtin_providers() -> Vec<Provider> {
    vec![
        Provider {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api: "openai-completions".into(),
            api_key: "$DEEPSEEK_API_KEY".into(),
            models: Some(vec![
                Model {
                    id: "deepseek-v4-flash".into(),
                    name: "DeepSeek V4 Flash".into(),
                    reasoning_efforts: vec!["off".into(), "high".into(), "max".into()],
                    input: vec!["text".into()],
                    context_window: 1_000_000,
                    max_tokens: 384000,
                    cost: Cost {
                        input: 1.0,
                        output: 2.0,
                        cache_read: 0.02,
                    },
                },
                Model {
                    id: "deepseek-v4-pro".into(),
                    name: "DeepSeek V4 Pro".into(),
                    reasoning_efforts: vec!["off".into(), "high".into(), "max".into()],
                    input: vec!["text".into()],
                    context_window: 1_000_000,
                    max_tokens: 384_000,
                    cost: Cost {
                        input: 3.0,
                        output: 6.0,
                        cache_read: 0.025,
                    },
                },
            ]),
        },
        Provider {
            id: "opencode-go".into(),
            name: "OpenCode Go".into(),
            base_url: "https://opencode.ai/zen/go/v1".into(),
            api: "openai-completions".into(),
            api_key: "$OPENCODE_API_KEY".into(),
            models: Some(vec![
                Model {
                    id: "deepseek-v4-flash".into(),
                    name: "DeepSeek V4 Flash".into(),
                    reasoning_efforts: vec!["off".into(), "high".into(), "max".into()],
                    input: vec!["text".into()],
                    context_window: 1_000_000,
                    max_tokens: 384_000,
                    cost: Cost {
                        input: 1.0,
                        output: 2.0,
                        cache_read: 0.02,
                    },
                },
                Model {
                    id: "deepseek-v4-pro".into(),
                    name: "DeepSeek V4 Pro".into(),
                    reasoning_efforts: vec!["off".into(), "high".into(), "max".into()],
                    input: vec!["text".into()],
                    context_window: 1_000_000,
                    max_tokens: 384_000,
                    cost: Cost {
                        input: 3.0,
                        output: 6.0,
                        cache_read: 0.025,
                    },
                },
            ]),
        },
    ]
}
