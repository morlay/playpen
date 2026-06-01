use crate::model::{Cost, Currency, InputType, Model, ModelProvider, ThinkingLevel};

pub fn providers() -> Vec<(&'static str, ModelProvider)> {
    vec![("deepseek", deepseek_provider())]
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
