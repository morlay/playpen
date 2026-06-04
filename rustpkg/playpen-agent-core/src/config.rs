use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::agent::settings::Settings;
use crate::model::Provider;
use crate::profile::Profile;
use crate::workspace::SandboxConfig;

#[derive(Clone, Default, serde::Serialize)]
pub struct AppConfig {
    pub settings: Settings,
    pub providers: HashMap<String, Provider>,
    pub sandbox: SandboxConfig,
    pub profiles: Vec<Profile>,
}

/// 展开字符串中的 `$ENV_VAR` 环境变量引用
pub fn expand_env_vars(s: &str) -> String {
    let re = regex::Regex::new(r"\$(\{)?([A-Za-z_][A-Za-z0-9_]*)\}?").unwrap();
    re.replace_all(s, |caps: &regex::Captures| {
        let var_name = caps.get(2).unwrap().as_str();
        std::env::var(var_name).unwrap_or_else(|_| caps.get(0).unwrap().as_str().to_string())
    })
    .to_string()
}

/// 从多个 toml::Value 合并 Settings。后加载的覆盖先加载的（None 不覆盖）。
pub fn merge_settings(values: &[toml::Value]) -> anyhow::Result<Settings> {
    let mut settings = Settings::default();
    for v in values {
        settings.apply_toml(v)?;
    }
    Ok(settings)
}

/// 从多个 toml::Value 合并 Providers。同 id 后加载覆盖先加载。
pub fn merge_providers(values: &[toml::Value]) -> anyhow::Result<HashMap<String, Provider>> {
    let mut providers = HashMap::new();
    for v in values {
        if let Some(table) = v.get("providers").and_then(|t| t.as_table()) {
            for (_pn, config) in table {
                match config.clone().try_into::<Provider>() {
                    Ok(pc) => {
                        providers.insert(pc.id.clone(), pc);
                    }
                    Err(e) => eprintln!("playpen: 解析 [providers] 配置失败，跳过：{}", e),
                }
            }
        }
    }
    for p in providers.values_mut() {
        p.api_key = expand_env_vars(&p.api_key);
    }
    Ok(providers)
}

/// 从多个 toml::Value 合并 Sandbox 配置。后加载覆盖先加载。
pub fn merge_sandbox(values: &[toml::Value]) -> anyhow::Result<SandboxConfig> {
    use merge::Merge;
    let mut config = SandboxConfig::default();
    for v in values {
        let sandbox_val = v.get("sandbox").unwrap_or(v);
        match sandbox_val.clone().try_into::<SandboxConfig>() {
            Ok(overlay) => config.merge(overlay),
            Err(e) => eprintln!("playpen: 解析 [sandbox] 配置失败，跳过：{}", e),
        }
    }
    Ok(config)
}

impl AppConfig {
    /// 从标准路径加载完整配置：
    /// 1. `~/.config/playpen/settings.toml`（全局配置）
    /// 2. `~/.config/playpen/conf.d/*.toml`（模块化配置）
    /// 3. `<cwd>/.playpen.toml`（项目级覆盖）
    pub fn load(cwd: &Path) -> anyhow::Result<Self> {
        let home = std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"));
        let config_dir = home.join(".config").join("playpen");

        let mut values = Vec::new();

        // 1. 全局 settings.toml
        let settings_path = config_dir.join("settings.toml");
        if settings_path.is_file() {
            match fs::read_to_string(&settings_path) {
                Ok(content) => {
                    let content = content.trim();
                    if !content.is_empty() {
                        match toml::from_str(content) {
                            Ok(v) => values.push(v),
                            Err(e) => {
                                eprintln!("playpen: 解析 {} 失败，跳过：{}", settings_path.display(), e)
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("playpen: 读取 {} 失败，跳过：{}", settings_path.display(), e)
                }
            }
        }

        // 2. conf.d 目录
        let conf_d = config_dir.join("conf.d");
        if conf_d.is_dir() {
            match load_conf_d(&conf_d) {
                Ok(mut v) => values.append(&mut v),
                Err(e) => eprintln!("playpen: 加载 conf.d 失败，跳过：{}", e),
            }
        }

        // 3. 项目级 .playpen.toml
        let project_path = cwd.join(".playpen.toml");
        if project_path.is_file() {
            match fs::read_to_string(&project_path) {
                Ok(content) => {
                    if !content.trim().is_empty() {
                        match toml::from_str(&content) {
                            Ok(v) => values.push(v),
                            Err(e) => {
                                eprintln!("playpen: 解析 {} 失败，跳过：{}", project_path.display(), e)
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("playpen: 读取 {} 失败，跳过：{}", project_path.display(), e)
                }
            }
        }

        // profiles
        let loader = crate::profile::Loader::new(cwd.to_path_buf());
        let profiles = loader.list_profiles().unwrap_or_default();

        Ok(Self {
            settings: merge_settings(&values)?,
            providers: merge_providers(&values)?,
            sandbox: merge_sandbox(&values)?,
            profiles,
        })
    }

    /// 加载配置，失败时降级为默认值。
    pub fn load_or_default(cwd: &Path) -> Self {
        match Self::load(cwd) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("playpen: 加载配置失败，使用默认值：{}", e);
                Self::default()
            }
        }
    }

    /// 从 conf.d 目录加载配置文件并合并。profiles 由调用方提供。
    pub fn load_from(conf_d_dir: &Path, profiles: Vec<Profile>) -> anyhow::Result<Self> {
        let values = load_conf_d(conf_d_dir)?;
        Ok(Self {
            settings: merge_settings(&values)?,
            providers: merge_providers(&values)?,
            sandbox: merge_sandbox(&values)?,
            profiles,
        })
    }
}

/// 扫描目录下所有 .toml 文件，按文件名排序后解析返回。单个文件解析失败时报错后跳过。
pub fn load_conf_d(dir: &Path) -> anyhow::Result<Vec<toml::Value>> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut entries: Vec<_> = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut values = Vec::new();
    for entry in &entries {
        let path = entry.path();
        match fs::read_to_string(&path) {
            Ok(content) => match toml::from_str(&content) {
                Ok(v) => values.push(v),
                Err(e) => {
                    eprintln!("playpen: 解析 {} 失败，跳过：{}", path.display(), e)
                }
            },
            Err(e) => eprintln!("playpen: 读取 {} 失败，跳过：{}", path.display(), e),
        }
    }
    Ok(values)
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
