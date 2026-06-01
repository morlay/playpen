#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Usage {
    pub input: usize,
    pub output: usize,
    pub cache_read: usize,
    pub cache_write: usize,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub enum Currency {
    #[default]
    CNY,
    USD,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub currency: Currency,
}

impl Default for Cost {
    fn default() -> Self {
        Self {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            currency: Currency::default(),
        }
    }
}

const COST_PER_MTOKEN: f64 = 1_000_000.0;

impl Cost {
    /// 按每 1M token 的单价计算费用。
    /// `input`、`output`、`cache_read` 的单位是 每 1M token 的价格。
    pub fn compute(&self, usage: &Usage) -> f64 {
        let input_cost = if self.cache_read > 0.0 {
            let cached = usage.cache_read.min(usage.input);
            (usage.input - cached) as f64 * self.input + cached as f64 * self.cache_read
        } else {
            usage.input as f64 * self.input
        };
        (input_cost + usage.output as f64 * self.output) / COST_PER_MTOKEN
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Off,
    High,
    Max,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    Text,
    Image,
    Audio,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelProvider {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<Model>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Model {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_efforts: Vec<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_types: Vec<InputType>,
    pub context_window: usize,
    pub max_tokens: usize,
    #[serde(default)]
    pub cost: Cost,
}

/// 模型 key 解析类型。
/// 格式：`{provider}/{model}`，如 `deepseek/deepseek-v4-flash`。
/// 无分隔符时默认 provider 为 `deepseek`。
#[derive(Debug, Clone)]
pub struct ModelKey {
    pub provider: String,
    pub model: String,
}

impl ModelKey {
    pub fn parse(key: &str) -> Self {
        match key.split_once('/') {
            Some((provider, model)) => Self {
                provider: provider.to_string(),
                model: model.to_string(),
            },
            None => Self {
                provider: "deepseek".to_string(),
                model: key.to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ModelProfile {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
}

#[cfg(test)]
#[path = "model_test.rs"]
mod tests;
