use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    pub input: usize,
    pub output: usize,
    pub cache_read: usize,
    pub cache_write: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cost {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
}

impl Cost {
    /// 根据 Usage 计算费用：input 扣除缓存命中后按 cache_read 单价计，其余按 input 单价。
    pub fn compute(&self, usage: &Usage) -> f64 {
        let input_cost = if self.cache_read > 0.0 {
            let cached = usage.cache_read.min(usage.input);
            (usage.input - cached) as f64 * self.input + cached as f64 * self.cache_read
        } else {
            usage.input as f64 * self.input
        };
        input_cost + usage.output as f64 * self.output
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Off,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api: String,
    pub api_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<Model>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub reasoning_efforts: Vec<String>,
    pub input: Vec<String>,
    pub context_window: usize,
    pub max_tokens: usize,
    #[serde(default)]
    pub cost: Cost,
}

impl Default for Cost {
    fn default() -> Self {
        Self { input: 0.0, output: 0.0, cache_read: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
    Text,
    Image,
    Audio,
}

#[cfg(test)] #[path = "model_test.rs"] mod tests;
