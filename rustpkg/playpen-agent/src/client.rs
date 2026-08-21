//! OpenAI-compatible LLM client，基于 rig-core。
//!
//! DeepSeek 路径使用自定义 provider 扩展 [`PlaypenDeepSeekExt`]：
//! 在 rig 内置 deepseek 行为的基础上，把 OpenAI 格式的 file 内容块
//! `{"type":"file","file":{"file_id":...}}` 改写为 DeepSeek Files API 期望的
//! `{"type":"file","file_id":...}`。

use playpen_config::Settings;
use playpen_config::model::ModelProfile;
use playpen_profile::AgentProfile;
use rig_core::client::{
    self, BearerAuth, Capabilities, Capable, CompletionClient, Nothing, Provider, ProviderBuilder,
};
use rig_core::completion::CompletionError;
use rig_core::http_client::{self, HttpClientExt};
use rig_core::providers::deepseek;
use rig_core::providers::openai;
use rig_core::providers::openai::completion::OpenAICompatibleProvider;

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

    /// 模型名是否以已知前缀开头 → 使用 DeepSeek 兼容流式协议。
    pub fn is_deepseek_compat(&self) -> bool {
        let model = self.model.split('/').next_back().unwrap_or("");
        model.starts_with("deepseek") || model.starts_with("glm") || model.starts_with("mimo")
    }

    /// 模型名是否为 DeepSeek 官方 provider（files API 上传仅对其启用）。
    pub fn is_deepseek_provider(&self) -> bool {
        self.model.split('/').next().unwrap_or("") == "deepseek"
    }

    /// 模型名是否为 vision 模型（Files API 的图片引用需要 vision 模型）。
    pub fn is_vision_model(&self) -> bool {
        self.model
            .split('/')
            .next_back()
            .unwrap_or("")
            .contains("vision")
    }

    /// 模型是否支持图片输入：优先按模型配置声明的 `input_types`，
    /// 未配置时按模型名（含 `vision`）兜底。
    pub fn supports_image(&self) -> bool {
        if let Some(mc) = &self.model_config {
            mc.input_types
                .contains(&playpen_config::model::InputType::Image)
        } else {
            self.is_vision_model()
        }
    }
}

// ── DeepSeek 自定义 provider ────────────────────────────────────────────

/// DeepSeek provider 扩展：在 rig 内置 deepseek 行为基础上改写 file 内容块。
#[derive(Debug, Default, Clone, Copy)]
pub struct PlaypenDeepSeekExt;

#[derive(Debug, Default, Clone, Copy)]
pub struct PlaypenDeepSeekExtBuilder;

impl Provider for PlaypenDeepSeekExt {
    type Builder = PlaypenDeepSeekExtBuilder;

    const VERIFY_PATH: &'static str = "/user/balance";
}

impl client::DebugExt for PlaypenDeepSeekExt {}

impl ProviderBuilder for PlaypenDeepSeekExtBuilder {
    type Extension<H>
        = PlaypenDeepSeekExt
    where
        H: HttpClientExt;
    type ApiKey = BearerAuth;

    const BASE_URL: &'static str = "https://api.deepseek.com";

    fn build<H>(
        _builder: &client::ClientBuilder<Self, Self::ApiKey, H>,
    ) -> http_client::Result<Self::Extension<H>>
    where
        H: HttpClientExt,
    {
        Ok(PlaypenDeepSeekExt)
    }
}

impl<H> Capabilities<H> for PlaypenDeepSeekExt {
    type Completion = Capable<openai::completion::GenericCompletionModel<PlaypenDeepSeekExt, H>>;
    type Embeddings = Nothing;
    type Transcription = Nothing;
    type ModelListing = Nothing;
    type Rerank = Nothing;
}

impl OpenAICompatibleProvider for PlaypenDeepSeekExt {
    const PROVIDER_NAME: &'static str = "deepseek";

    type StreamingUsage = deepseek::Usage;

    // DeepSeek 的 API 只支持 `json_object` response formats（经 additional_params 传入），
    // 不支持 `json_schema` 映射。
    const SUPPORTS_RESPONSE_FORMAT: bool = false;

    // DeepSeek 可以在单个 streaming chunk 里发出完整的 tool call。
    const EMITS_COMPLETE_SINGLE_CHUNK_TOOL_CALLS: bool = true;

    type Response = deepseek::CompletionResponse;

    fn finalize_request_body(&self, body: &mut serde_json::Value) -> Result<(), CompletionError> {
        // 1. rig 内置 deepseek 行为：content 数组平铺 / tool_calls index / tool_choice 抑制。
        deepseek_finalize_request_body(body)?;

        // 2. file 内容块改写：OpenAI 格式 → DeepSeek 格式。
        rewrite_file_blocks(body);

        Ok(())
    }
}

/// rig `providers::deepseek` 内置的 finalize 逻辑（内容平铺 + index + tool_choice 抑制）。
/// 与 rig-core 0.42 的 DeepSeekExt 实现保持一致。
fn deepseek_finalize_request_body(body: &mut serde_json::Value) -> Result<(), CompletionError> {
    let Some(map) = body.as_object_mut() else {
        return Ok(());
    };

    // DeepSeek takes message `content` as a plain string, not an array of
    // content parts, and echoes tool calls back with an `index` field.
    if let Some(messages) = map
        .get_mut("messages")
        .and_then(serde_json::Value::as_array_mut)
    {
        for message in messages {
            let Some(message) = message.as_object_mut() else {
                continue;
            };
            let is_assistant =
                message.get("role").and_then(serde_json::Value::as_str) == Some("assistant");

            if let Some(content) = message.get_mut("content") {
                let separator = if is_assistant { "" } else { "\n" };
                // Text-only arrays flatten; an array carrying an image, audio,
                // video or file part is left alone so DeepSeek's own rejection
                // reaches the caller.
                flatten_text_content_parts(content, separator, true);
            } else if is_assistant && !message.contains_key("content") {
                // Tool-call-only assistant turns must still carry an
                // (empty) string content field.
                message.insert(
                    "content".to_string(),
                    serde_json::Value::String(String::new()),
                );
            }

            if is_assistant
                && let Some(tool_calls) = message
                    .get_mut("tool_calls")
                    .and_then(serde_json::Value::as_array_mut)
            {
                for tool_call in tool_calls {
                    if let Some(tool_call) = tool_call.as_object_mut() {
                        tool_call
                            .entry("index")
                            .or_insert_with(|| serde_json::json!(0));
                    }
                }
            }
        }
    }

    // DeepSeek rejects forced tool choices (`required` or a specific
    // function) unless thinking is explicitly disabled; suppress them to
    // an explicit `null` otherwise.
    let thinking_disabled = map
        .get("thinking")
        .and_then(|thinking| thinking.get("type"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|mode| mode.eq_ignore_ascii_case("disabled"));
    if !thinking_disabled && let Some(tool_choice) = map.get_mut("tool_choice") {
        let forced = tool_choice.is_object() || tool_choice.as_str() == Some("required");
        if forced {
            *tool_choice = serde_json::Value::Null;
        }
    }

    Ok(())
}

/// 与 rig `openai::completion::flatten_text_content_parts` 同逻辑的本地实现
/// （rig 侧为 `pub(crate)`，playpen 无法直接调用）：
/// content-part 数组平铺为文本字符串；`only_if_all_text` 为 true 时，
/// 含非文本部分（图片/音频/视频/file 块）的数组保留不动。
fn flatten_text_content_parts(
    content: &mut serde_json::Value,
    separator: &str,
    only_if_all_text: bool,
) {
    fn part_text(part: &serde_json::Value) -> Option<&str> {
        part.get("text")
            .and_then(serde_json::Value::as_str)
            .or_else(|| part.get("refusal").and_then(serde_json::Value::as_str))
    }

    let Some(parts) = content.as_array() else {
        return;
    };
    if only_if_all_text && !parts.iter().all(|part| part_text(part).is_some()) {
        return;
    }
    let mut flattened = String::new();
    for text in parts.iter().filter_map(part_text) {
        if !flattened.is_empty() {
            flattened.push_str(separator);
        }
        flattened.push_str(text);
    }
    *content = serde_json::Value::String(flattened);
}

/// 将 OpenAI 格式的 file 内容块改写为 DeepSeek Files API 格式：
///
/// ```json
/// {"type":"file","file":{"file_id":"file-api-..."}}
/// ```
/// →
/// ```json
/// {"type":"file","file_id":"file-api-..."}
/// ```
fn rewrite_file_blocks(body: &mut serde_json::Value) {
    let Some(messages) = body
        .get_mut("messages")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for message in messages {
        let Some(content) = message
            .get_mut("content")
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for part in content {
            let Some(obj) = part.as_object_mut() else {
                continue;
            };
            if obj.get("type").and_then(serde_json::Value::as_str) != Some("file") {
                continue;
            }
            let Some(file) = obj.remove("file") else {
                continue;
            };
            if let Some(file_id) = file.get("file_id") {
                obj.insert("file_id".into(), file_id.clone());
            }
            if let Some(file_data) = file.get("file_data") {
                obj.insert("file_data".into(), file_data.clone());
            }
            if let Some(filename) = file.get("filename") {
                obj.insert("filename".into(), filename.clone());
            }
        }
    }
}

/// 统一模型枚举，消除 `is_deepseek()` 分支。
pub enum ModelEnum {
    Deepseek {
        model: PlaypenDeepSeekModel,
    },
    Openai {
        model: rig_core::providers::openai::GenericCompletionModel,
    },
}

/// 自定义 DeepSeek provider 的 completion 模型类型。
pub type PlaypenDeepSeekModel =
    openai::completion::GenericCompletionModel<PlaypenDeepSeekExt, reqwest::Client>;

/// OpenAI-compatible LLM 客户端。
pub struct LlmClient {
    config: LlmConfig,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Self {
        Self { config }
    }

    /// 客户端配置访问器。
    pub fn config(&self) -> &LlmConfig {
        &self.config
    }

    /// 根据 config 构建对应 provider 的模型，统一返回枚举。
    pub fn build_model(&self) -> anyhow::Result<ModelEnum> {
        if self.config.is_deepseek_compat() {
            self.build_deepseek_model()
                .map(|model| ModelEnum::Deepseek { model })
        } else {
            self.build_openai_model()
                .map(|model| ModelEnum::Openai { model })
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
            if self.config.is_deepseek_compat() {
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

    /// 通过自定义 DeepSeek provider 构建模型（支持 thinking token 流式解析 + file 块改写）。
    pub fn build_deepseek_model(&self) -> anyhow::Result<PlaypenDeepSeekModel> {
        let client = client::Client::<PlaypenDeepSeekExt, reqwest::Client>::builder()
            .base_url(&self.config.base_url)
            .api_key(BearerAuth::from(self.config.api_key.clone()))
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
