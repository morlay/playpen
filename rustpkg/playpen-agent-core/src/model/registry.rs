use std::collections::HashMap;

use rig_core::providers::openai::{self, CompletionsClient};

use crate::model::{Model, Provider};
use crate::config::expand_env_vars;

pub struct Registry {
    providers: HashMap<String, Provider>,
}

impl Registry {
    pub fn new(providers: HashMap<String, Provider>) -> Self {
        Self { providers }
    }

    pub fn list_models(&self) -> Vec<Model> {
        let mut models = Vec::new();
        for provider in self.providers.values() {
            if let Some(ref provider_models) = provider.models {
                for m in provider_models.clone() {
                    models.push(m);
                }
            }
        }
        models
    }

    /// 列出所有模型，附带 provider_id。
    pub fn list_models_with_provider(&self) -> Vec<(String, Model)> {
        let mut models = Vec::new();
        for (provider_id, provider) in &self.providers {
            if let Some(ref provider_models) = provider.models {
                for m in provider_models.clone() {
                    models.push((provider_id.clone(), m));
                }
            }
        }
        models
    }

    pub fn find_model(&self, provider_id: &str, model_id: &str) -> Option<Model> {
        let provider = self.providers.get(provider_id)?;
        provider.models.as_ref()?.iter().find(|m| m.id == model_id).cloned()
    }

    pub fn build_client(&self, provider_id: &str) -> anyhow::Result<CompletionsClient> {
        let provider = self.providers.get(provider_id)
            .ok_or_else(|| anyhow::anyhow!("Provider 不存在: {}", provider_id))?;
        let api_key = expand_env_vars(&provider.api_key);
        Ok(openai::CompletionsClient::builder()
            .api_key(&api_key)
            .base_url(&provider.base_url)
            .build()?)
    }
}

#[cfg(test)]
#[path = "registry_test.rs"]
mod tests;
