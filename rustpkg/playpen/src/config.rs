// 入口层配置加载。
// 负责文件发现、读取，委托 agent-core 做合并逻辑。

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

// ── Zed agent ──

#[derive(Deserialize)]
struct PlaypenToml {
    agents: Option<AgentSections>,
}

#[derive(Deserialize)]
struct AgentSections {
    zed: Option<playpen_zed_agent::agent::ZedAgentConfig>,
}

pub fn load_zed_agent_config() -> Option<playpen_zed_agent::agent::ZedAgentConfig> {
    let path = home_dir().join(".config/playpen.toml");
    let raw = fs::read_to_string(&path).ok()?;
    let config: PlaypenToml = toml::from_str(&raw).ok()?;
    config.agents.and_then(|a| a.zed)
}

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}
