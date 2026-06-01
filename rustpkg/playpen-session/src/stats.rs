use std::collections::HashMap;

use playpen_content::{Event, TokenUsage};

/// Session 级别统计。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionStats {
    pub token_usage: Option<TokenUsage>,
    pub tool_calls: ToolCallStats,
    pub turns: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCallStats {
    pub total: usize,
    pub by_name: HashMap<String, usize>,
}

impl SessionStats {
    /// 从事件列表计算统计。
    pub fn from_events(events: &[Event]) -> Self {
        let mut token_usage: Option<TokenUsage> = None;
        let mut tool_by_name: HashMap<String, usize> = HashMap::new();
        let mut turns: usize = 0;

        for event in events {
            match event {
                Event::TurnStop {
                    token_usage: usage, ..
                } => {
                    // 取最后一个 TurnStop 的 token_usage
                    if let Some(u) = usage {
                        token_usage = Some(TokenUsage {
                            prompt_token_count: token_usage
                                .as_ref()
                                .map(|t| t.prompt_token_count)
                                .unwrap_or(0)
                                + u.prompt_token_count,
                            candidates_token_count: token_usage
                                .as_ref()
                                .map(|t| t.candidates_token_count)
                                .unwrap_or(0)
                                + u.candidates_token_count,
                            total_token_count: token_usage
                                .as_ref()
                                .map(|t| t.total_token_count)
                                .unwrap_or(0)
                                + u.total_token_count,
                            cache_read_input_token_count: merge_opt(
                                &token_usage,
                                |t| t.cache_read_input_token_count,
                                u.cache_read_input_token_count,
                            ),
                            cache_creation_input_token_count: merge_opt(
                                &token_usage,
                                |t| t.cache_creation_input_token_count,
                                u.cache_creation_input_token_count,
                            ),
                            thinking_token_count: merge_opt(
                                &token_usage,
                                |t| t.thinking_token_count,
                                u.thinking_token_count,
                            ),
                        });
                    }
                    turns += 1;
                }
                Event::FunctionCall { name, .. } => {
                    *tool_by_name.entry(name.clone()).or_default() += 1;
                }
                _ => {}
            }
        }

        Self {
            token_usage,
            tool_calls: ToolCallStats {
                total: tool_by_name.values().sum(),
                by_name: tool_by_name,
            },
            turns,
        }
    }
}

fn merge_opt(
    existing: &Option<TokenUsage>,
    f: impl FnOnce(&TokenUsage) -> Option<i32>,
    new: Option<i32>,
) -> Option<i32> {
    let existing_val = existing.as_ref().and_then(f);
    match (existing_val, new) {
        (Some(a), Some(b)) => Some(a + b),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
