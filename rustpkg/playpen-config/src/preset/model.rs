use crate::model::{Cost, Currency, InputType, Model, ModelProvider, ThinkingLevel};

pub fn providers() -> Vec<(&'static str, ModelProvider)> {
    vec![("deepseek", deepseek_provider()), ("mimo", mimo_provider())]
}

fn deepseek_provider() -> ModelProvider {
    ModelProvider {
        name: "DeepSeek".into(),
        base_url: "https://api.deepseek.com".into(),
        api_key: "${DEEPSEEK_API_KEY}".into(),
        models: Some(vec![
            Model {
                name: "deepseek-v4-flash".into(),
                display_name: Some("DeepSeek V4 Flash".into()),
                reasoning_efforts: vec![
                    ThinkingLevel::Off,
                    ThinkingLevel::High,
                    ThinkingLevel::Max,
                ],
                input_types: vec![InputType::Text],
                context_window: 1_000_000,
                max_tokens: 384_000,
                cost: Cost {
                    input: 1.0,
                    output: 2.0,
                    cache_read: 0.02,
                    currency: Currency::CNY,
                },
            },
            Model {
                name: "deepseek-v4-pro".into(),
                display_name: Some("DeepSeek V4 Pro".into()),
                reasoning_efforts: vec![
                    ThinkingLevel::Off,
                    ThinkingLevel::High,
                    ThinkingLevel::Max,
                ],
                input_types: vec![InputType::Text],
                context_window: 1_000_000,
                max_tokens: 384_000,
                cost: Cost {
                    input: 2.0,
                    output: 6.0,
                    cache_read: 0.025,
                    currency: Currency::CNY,
                },
            },
        ]),
    }
}

fn mimo_provider() -> ModelProvider {
    ModelProvider {
        name: "MiMo".into(),
        base_url: "https://api.xiaomimimo.com/v1".into(),
        api_key: "${MIMO_API_KEY}".into(),
        models: Some(vec![
            Model {
                name: "mimo-v2.5".into(),
                display_name: Some("MiMo V2.5".into()),
                reasoning_efforts: vec![
                    ThinkingLevel::Off,
                    ThinkingLevel::High,
                    ThinkingLevel::Max,
                ],
                input_types: vec![InputType::Text],
                context_window: 1_000_000,
                max_tokens: 128_000,
                cost: Cost {
                    input: 1.0,
                    output: 2.0,
                    cache_read: 0.02,
                    currency: Currency::CNY,
                },
            },
            Model {
                name: "mimo-v2.5-pro".into(),
                display_name: Some("MiMo V2.5 Pro".into()),
                reasoning_efforts: vec![
                    ThinkingLevel::Off,
                    ThinkingLevel::High,
                    ThinkingLevel::Max,
                ],
                input_types: vec![InputType::Text],
                context_window: 1_000_000,
                max_tokens: 128_000,
                cost: Cost {
                    input: 2.0,
                    output: 6.0,
                    cache_read: 0.025,
                    currency: Currency::CNY,
                },
            },
        ]),
    }
}
