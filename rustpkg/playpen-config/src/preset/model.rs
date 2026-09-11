use crate::model::{Cost, Currency, InputType, Model, ModelProvider, ThinkingLevel};

pub fn providers() -> Vec<(&'static str, ModelProvider)> {
    vec![
        ("deepseek", deepseek_provider()),
        ("commandcode", commandcode_provider()),
        ("ollma", ollma_provider()),
    ]
}

fn ollma_provider() -> ModelProvider {
    ModelProvider {
        name: "Ollma".into(),
        base_url: "https://ollama.com/v1".into(),
        api_key: "${OLLAMA_API_KEY}".into(),
        models: Some(vec![Model {
            name: "deepseek-v4.1-flash".into(),
            display_name: Some("DeepSeek V4.1 Flash @ Ollma".into()),
            reasoning_efforts: vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ],
            input_types: vec![InputType::Text, InputType::Image],
            context_window: 1_000_000,
            max_tokens: 384_000,
            cost: Cost {
                input: 1.0,
                output: 4.0,
                cache_read: 0.02,
                currency: Currency::CNY,
            },
        }]),
    }
}

fn commandcode_provider() -> ModelProvider {
    ModelProvider {
        name: "Command Code".into(),
        base_url: "https://api.commandcode.ai/provider/v1".into(),
        api_key: "${COMMANDCODE_API_KEY}".into(),
        models: Some(vec![Model {
            name: "deepseek/deepseek-v4.1-flash".into(),
            display_name: Some("DeepSeek V4.1 Flash @ Command Code".into()),
            reasoning_efforts: vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ],
            input_types: vec![InputType::Text, InputType::Image],
            context_window: 1_000_000,
            max_tokens: 384_000,
            cost: Cost {
                input: 1.0,
                output: 4.0,
                cache_read: 0.02,
                currency: Currency::CNY,
            },
        }]),
    }
}

fn deepseek_provider() -> ModelProvider {
    ModelProvider {
        name: "DeepSeek".into(),
        base_url: "https://api.deepseek.com".into(),
        api_key: "${DEEPSEEK_API_KEY}".into(),
        models: Some(vec![Model {
            name: "deepseek-flash".into(),
            display_name: Some("DeepSeek V4.1 Flash @ DeepSeek".into()),
            reasoning_efforts: vec![
                ThinkingLevel::Off,
                ThinkingLevel::Low,
                ThinkingLevel::High,
                ThinkingLevel::Max,
            ],
            input_types: vec![InputType::Text, InputType::Image],
            context_window: 1_000_000,
            max_tokens: 384_000,
            cost: Cost {
                input: 1.0,
                output: 4.0,
                cache_read: 0.02,
                currency: Currency::CNY,
            },
        }]),
    }
}
