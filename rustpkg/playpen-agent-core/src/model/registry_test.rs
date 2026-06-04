use crate::model::Provider;

use super::*;

fn test_providers() -> Vec<Provider> {
    vec![
        Provider {
                    id: "openai".into(), name: "openai".into(),
                    api: "openai-completions".into(), base_url: "https://api.openai.com/v1".into(),
                    api_key: "sk-test".into(),
            models: Some(vec![
                crate::model::Model {
                    id: "gpt-4o".into(), name: "GPT-4o".into(),
                    reasoning_efforts: vec!["off".into()], input: vec!["text".into()],
                    context_window: 128000, max_tokens: 4096,
                    cost: Default::default(),
                },
            ]),
        },
    ]
}

#[test]
fn registry_list_models() {
    let providers: std::collections::HashMap<String, Provider> =
        test_providers().into_iter().map(|p| (p.id.clone(), p)).collect();
    let reg = Registry::new(providers);
    let models = reg.list_models();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "gpt-4o");
}

#[test]
fn registry_find_model() {
    let providers: std::collections::HashMap<String, Provider> =
        test_providers().into_iter().map(|p| (p.id.clone(), p)).collect();
    let reg = Registry::new(providers);
    assert!(reg.find_model("openai", "gpt-4o").is_some());
    assert!(reg.find_model("openai", "nonexistent").is_none());
    assert!(reg.find_model("unknown", "gpt-4o").is_none());
}
