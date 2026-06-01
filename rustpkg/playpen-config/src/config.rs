use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::model::{Model, ModelProvider};

use crate::preset;

#[derive(Debug, Clone)]
pub struct Dirs {
    pub working_dir: PathBuf,
    pub config_data_dir: PathBuf,
    pub agents_dir: PathBuf,
}

impl Dirs {
    pub fn with_defaults(cwd: &Path) -> Self {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        Self {
            working_dir: cwd.to_path_buf(),
            config_data_dir: home.join(".config").join("playpen"),
            agents_dir: home.join(".agents"),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct SandboxProfile {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<AccessSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filesystem: Option<AccessSection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellSection>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AccessSection {
    #[serde(default)]
    pub access: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShellSection {
    pub allow_pipe: Option<bool>,
    pub allow_multiple: Option<bool>,
    #[serde(default)]
    pub allow: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxProfile>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub model_providers: HashMap<String, ModelProvider>,
}

impl Settings {
    /// 根据 model key（如 "deepseek/deepseek-v4-flash"）查找完整的 Model 定义。
    pub fn find_model(&self, model_key: &str) -> Option<&Model> {
        let (provider_id, model_name) = model_key.split_once('/')?;
        let provider = self.model_providers.get(provider_id)?;
        let models = provider.models.as_ref()?;
        models.iter().find(|m| m.name == model_name)
    }
}

#[derive(Clone, Default, serde::Serialize)]
pub struct AppConfig {
    pub settings: Settings,
    pub sandbox: playpen_sandbox::config::Config,
}

impl AppConfig {
    pub fn load(cwd: &Path) -> anyhow::Result<Self> {
        let dirs = Dirs::with_defaults(cwd);
        let mut values = Vec::new();
        Self::load_file(&mut values, &dirs.config_data_dir.join("settings.toml"));
        Self::load_conf_d(&mut values, &dirs.config_data_dir.join("conf.d"))?;
        Self::load_file(&mut values, &cwd.join(".playpen.toml"));

        let settings = merge_settings(&values)?;
        let sandbox = merge_sandbox(&values)?;

        Ok(Self { settings, sandbox })
    }

    pub fn load_or_default(cwd: &Path) -> Self {
        Self::load(cwd).unwrap_or_default()
    }

    fn load_file(values: &mut Vec<toml::Value>, path: &Path) {
        if !path.is_file() {
            return;
        }
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path=%path.display(), error=%e, "读取配置失败");
                return;
            }
        };
        if content.trim().is_empty() {
            return;
        }
        match toml::from_str(&content) {
            Ok(v) => values.push(v),
            Err(e) => tracing::warn!(path=%path.display(), error=%e, "解析配置失败"),
        }
    }

    fn load_conf_d(values: &mut Vec<toml::Value>, dir: &Path) -> anyhow::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in &entries {
            Self::load_file(values, &entry.path());
        }
        Ok(())
    }
}

pub fn expand_env_vars(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' {
            if chars.peek() == Some(&'{') {
                chars.next();
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    name.push(c);
                }
                result.push_str(&std::env::var(&name).unwrap_or_else(|_| format!("${{{name}}}")));
            } else {
                let mut name = String::new();
                for c in chars.by_ref() {
                    if !c.is_alphanumeric() && c != '_' {
                        break;
                    }
                    name.push(c);
                }
                if !name.is_empty() {
                    result.push_str(&std::env::var(&name).unwrap_or_else(|_| format!("${name}")));
                } else {
                    result.push('$');
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn merge_settings(values: &[toml::Value]) -> anyhow::Result<Settings> {
    let mut settings = Settings::default();

    // 预设 providers 作为默认值，用户配置可覆盖
    for (key, mut provider) in preset::providers() {
        provider.api_key = expand_env_vars(&provider.api_key);
        settings.model_providers.insert(key.to_string(), provider);
    }

    for v in values {
        if let Ok(overlay) = v.clone().try_into::<Settings>() {
            if overlay.default_profile.is_some() {
                settings.default_profile = overlay.default_profile;
            }
            if overlay.sandbox.is_some() {
                settings.sandbox = overlay.sandbox;
            }
            for (k, v) in overlay.model_providers {
                let mut p = v;
                p.api_key = expand_env_vars(&p.api_key);
                settings.model_providers.insert(k, p);
            }
        }
    }
    Ok(settings)
}

pub fn merge_sandbox(values: &[toml::Value]) -> anyhow::Result<playpen_sandbox::config::Config> {
    use merge::Merge;
    let mut config = playpen_sandbox::config::Config::default();
    for v in values {
        let sandbox_val = v.get("sandbox").unwrap_or(v);
        match sandbox_val
            .clone()
            .try_into::<playpen_sandbox::config::Config>()
        {
            Ok(overlay) => config.merge(overlay),
            Err(e) => tracing::warn!(error=%e, "解析 [sandbox] 配置失败，跳过"),
        }
    }
    Ok(config)
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
