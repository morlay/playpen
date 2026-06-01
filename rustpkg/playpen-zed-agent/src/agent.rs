use serde::Deserialize;
use std::fs;

use crate::tools;

/// zed agent 配置（来自 playpen.toml 的 [agents.zed]）
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZedAgentConfig {
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub guidelines: Option<Vec<String>>,
}

/// 生成 AGENTS.md 内容。write 为 true 时写入文件，否则输出到 stdout。
pub fn generate_agents_md(
    config: &ZedAgentConfig,
    path: &std::path::Path,
    write: bool,
) -> anyhow::Result<()> {
    let mut guidelines: Vec<String> = Vec::new();

    for tool in tools::TOOLS {
        let guideline = if tool.enabled {
            tool.prompt_guideline
        } else {
            tool.prompt_guideline_disabled
        };
        if let Some(g) = guideline {
            for line in g.trim().lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    guidelines.push(trimmed.to_string());
                }
            }
        }
    }
    if let Some(ref config_guidelines) = config.guidelines {
        for g in config_guidelines {
            let trimmed = g.trim().to_string();
            if !trimmed.is_empty() && !guidelines.contains(&trimmed) {
                guidelines.push(trimmed);
            }
        }
    }

    let mut content = String::new();
    if let Some(ref system) = config.system {
        content.push_str(system.trim());
        content.push_str("\n\n");
    }

    if !guidelines.is_empty() {
        content.push_str("## 使用指南\n");
        for g in &guidelines {
            content.push_str(&format!("- {}\n", g));
        }
    }

    if write {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content.trim_end())?;
        eprintln!("已写入: {}", path.display());
    } else {
        println!("// === AGENTS.md ===");
        println!("{}", content.trim_end());
    }
    Ok(())
}
