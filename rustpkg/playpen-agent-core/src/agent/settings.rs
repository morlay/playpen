use serde::{Deserialize, Serialize};

use crate::model::ThinkingLevel;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_thinking_level: Option<ThinkingLevel>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,
}


impl Settings {
    /// 从 toml::Value 反序列化后合并到当前实例（None 不覆盖）
    pub fn apply_toml(&mut self, value: &toml::Value) -> anyhow::Result<()> {
        let overlay: Settings = match value.clone().try_into() {
            Ok(s) => s,
            Err(_e) => {
                // 容错：配置不完整时跳过，使用默认值
                return Ok(());
            }
        };
        if let Some(v) = overlay.default_provider { self.default_provider = Some(v); }
        if let Some(v) = overlay.default_model { self.default_model = Some(v); }
        if let Some(v) = overlay.default_thinking_level { self.default_thinking_level = Some(v); }
        if let Some(v) = overlay.default_agent { self.default_agent = Some(v); }
        if overlay.retry.is_some() { self.retry = overlay.retry; }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_delay_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_multiplier: Option<f64>,
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
