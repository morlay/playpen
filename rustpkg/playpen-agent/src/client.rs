//! OpenAI-compatible LLM client，基于 rig-core。

use playpen_config::Settings;
use playpen_config::model::ModelProfile;
use playpen_profile::AgentProfile;
use rig_core::client::CompletionClient;

/// LLM 客户端配置。
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 当前模型的完整配置（如 max_tokens），可选
    pub model_config: Option<playpen_config::model::Model>,
}

impl LlmConfig {
    pub fn from_settings(settings: &Settings, profile: &dyn AgentProfile) -> anyhow::Result<Self> {
        let mp = profile.model_profile();
        let mk = playpen_config::model::ModelKey::parse(&mp.model);

        let provider = settings
            .model_providers
            .get(&mk.provider)
            .ok_or_else(|| anyhow::anyhow!("provider {} not configured", mk.provider))?;

        let model_config = provider
            .models
            .as_ref()
            .and_then(|models| models.iter().find(|m| m.name == mk.model))
            .cloned();

        Ok(Self {
            base_url: provider.base_url.trim_end_matches('/').to_string(),
            api_key: provider.api_key.clone(),
            model: mk.model.clone(),
            model_config,
        })
    }

    /// 检查是否为 DeepSeek 模型（需要特殊流式处理）
    pub fn is_deepseek(&self) -> bool {
        self.model
            .split('/')
            .next_back()
            .unwrap_or("")
            .starts_with("deepseek")
    }
}

/// 统一模型枚举，消除 `is_deepseek()` 分支。
/// 每个变体携带对应的 finish_reason 提取器。
pub enum ModelEnum {
    Deepseek {
        model: rig_core::providers::deepseek::CompletionModel,
        extract_finish_reason: fn(&dyn std::any::Any) -> Option<String>,
    },
    Openai {
        model: rig_core::providers::openai::GenericCompletionModel,
        extract_finish_reason: fn(&dyn std::any::Any) -> Option<String>,
    },
}

impl ModelEnum {
    /// 从 Final response 中提取 finish_reason。
    pub fn extract_finish_reason(&self, response: &dyn std::any::Any) -> Option<String> {
        match self {
            ModelEnum::Deepseek {
                extract_finish_reason,
                ..
            }
            | ModelEnum::Openai {
                extract_finish_reason,
                ..
            } => extract_finish_reason(response),
        }
    }
}

/// OpenAI-compatible LLM 客户端。
pub struct LlmClient {
    config: LlmConfig,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        Self { config }
    }

    /// 根据 config 构建对应 provider 的模型，统一返回枚举。
    pub fn build_model(&self) -> anyhow::Result<ModelEnum> {
        if self.config.is_deepseek() {
            self.build_deepseek_model()
                .map(|model| ModelEnum::Deepseek {
                    extract_finish_reason: |response| {
                        response
                            .downcast_ref::<rig_core::providers::deepseek::CompletionResponse>()
                            .and_then(|r| r.choices.first().map(|c| c.finish_reason.clone()))
                    },
                    model,
                })
        } else {
            self.build_openai_model().map(|model| ModelEnum::Openai {
                extract_finish_reason: |response| {
                    response
                        .downcast_ref::<rig_core::providers::openai::completion::CompletionResponse>()
                        .and_then(|r| r.choices.first().map(|c| c.finish_reason.clone()))
                },
                model,
            })
        }
    }

    /// 构建 provider-specific additional_params。
    /// 根据 provider 类型（deepseek / openai-compatible）生成对应的 thinking/reasoning_effort 参数。
    pub fn build_additional_params(
        &self,
        model_profile: &ModelProfile,
        model_max_tokens: Option<usize>,
    ) -> Option<serde_json::Value> {
        let mut params = serde_json::Map::new();

        if let Some(tp) = model_profile.top_p {
            params.insert("top_p".into(), serde_json::json!(tp));
        }

        if let Some(max_tokens) = model_max_tokens {
            params.insert("max_tokens".into(), serde_json::json!(max_tokens));
        }

        if let Some(ref tl) = model_profile.thinking_level {
            if self.config.is_deepseek() {
                let thinking_type = match tl {
                    playpen_config::model::ThinkingLevel::Off => "disabled",
                    _ => "enabled",
                };
                params.insert(
                    "thinking".into(),
                    serde_json::json!({ "type": thinking_type }),
                );
            } else {
                // OpenAI / 兼容 provider: reasoning_effort
                params.insert("reasoning_effort".into(), serde_json::to_value(tl).unwrap());
            }
        }

        if params.is_empty() {
            None
        } else {
            Some(serde_json::Value::Object(params))
        }
    }

    /// 通过 deepseek provider 构建模型（支持 thinking token 流式解析）。
    pub fn build_deepseek_model(
        &self,
    ) -> anyhow::Result<rig_core::providers::deepseek::CompletionModel> {
        use rig_core::providers::deepseek::Client as DeepSeekClient;

        let client = DeepSeekClient::builder()
            .base_url(&self.config.base_url)
            .api_key(&self.config.api_key)
            .build()
            .map_err(|e| anyhow::anyhow!("build deepseek client failed: {e}"))?;

        Ok(client.completion_model(&self.config.model))
    }

    /// 通过 OpenAI-compatible provider 构建模型。
    pub fn build_openai_model(
        &self,
    ) -> anyhow::Result<rig_core::providers::openai::GenericCompletionModel> {
        let client = rig_core::providers::openai::Client::builder()
            .api_key(&self.config.api_key)
            .base_url(&self.config.base_url)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to create OpenAI client: {e}"))?;

        Ok(client
            .completions_api()
            .completion_model(&self.config.model))
    }
}

#[cfg(test)]
#[path = "client_test.rs"]
mod tests;
