use playpen_content::{ContentBlock, Event, StopReason};
use serde_json::Value;

/// 将 Event 映射为角色字符串（对应 DB `role` 列）。
pub(crate) fn event_role(event: &Event) -> &'static str {
    match event {
        Event::UserMessage { .. } => "user",
        Event::ModelMessage { .. }
        | Event::ModelMessageDelta { .. }
        | Event::ModelThought { .. }
        | Event::ModelThoughtDelta { .. } => "model",
        Event::FunctionCall { .. }
        | Event::FunctionResult { .. }
        | Event::FunctionOutputDelta { .. } => "function",
        Event::TurnStop { .. } => "turn",
        Event::StateUpdate { .. } => "state",
    }
}

/// 归一化后的事件结构，用于写入 DB。
pub(crate) struct Normalized {
    pub role: &'static str,
    pub kind: &'static str,
    pub name: Option<String>,
    /// role 为 function 时存 call_id
    pub action_id: String,
    pub payload: serde_json::Value,
}

/// 将 Event 归一化为 DB 行数据。Delta 和部分事件返回 None。
pub(crate) fn normalize_event(event: &Event) -> Option<Normalized> {
    let role = event_role(event);
    let (kind, name, action_id, payload) = match event {
        // ── 不入库的 Delta ──
        Event::ModelMessageDelta { .. }
        | Event::ModelThoughtDelta { .. }
        | Event::FunctionOutputDelta { .. } => return None,

        // ── StateUpdate ──
        Event::StateUpdate { name, data, .. } => ("state_update", Some(name.clone()), String::new(), data.clone()),

        // ── TurnStop ──
        Event::TurnStop {
            stop_reason,
            token_usage,
            ..
        } => {
            let mut payload = serde_json::json!({"stop_reason": stop_reason});
            if let Some(usage) = token_usage {
                payload["token_usage"] = serde_json::to_value(usage).unwrap();
            }
            ("stop_reason", None, String::new(), payload)
        }

        // ── UserMessage / ModelMessage ──
        Event::UserMessage { id, content } => (
            "message",
            None,
            String::new(),
            serde_json::json!({"role": "user", "content": content, "id": id}),
        ),
        Event::ModelMessage { id, content } => (
            "message",
            None,
            String::new(),
            serde_json::json!({"role": "model", "content": content, "id": id}),
        ),

        // ── ModelThought ──
        Event::ModelThought { id, text } => (
            "thinking",
            None,
            String::new(),
            serde_json::json!({"text": text, "id": id}),
        ),

        // ── FunctionCall ──
        Event::FunctionCall {
            call_id: id,
            name,
            args,
            ..
        } => {
            let n = name.clone();
            let a = id.clone();
            (
                "function_call",
                Some(n),
                a,
                serde_json::json!({"id": id, "args": args}),
            )
        }

        // ── FunctionResult ──
        Event::FunctionResult {
            call_id: cid,
            name,
            content,
            code,
            ..
        } => {
            let n = name.clone();
            let a = cid.clone();
            (
                "function_result",
                Some(n),
                a,
                serde_json::json!({"call_id": cid, "content": content, "code": code}),
            )
        }
    };

    Some(Normalized {
        kind,
        role,
        name,
        action_id,
        payload,
    })
}

/// 从 DB 行数据反归一化为 Event 序列。
pub(crate) fn denormalize_events(
    kind: &str,
    role: &str,
    name: Option<&str>,
    data: &[u8],
    event_id: &str,
) -> Vec<Event> {
    let decompressed = zstd::decode_all(std::io::Cursor::new(data)).expect("zstd 解压不应失败");
    let value: Value = serde_json::from_slice(&decompressed).expect("JSON 反序列化不应失败");

    macro_rules! get_id {
        () => {
            value["id"]
                .as_str()
                .filter(|s| !s.is_empty() && *s != "null")
                .map(|s| s.to_string())
                .unwrap_or_else(|| event_id.to_string())
        };
    }

    match role {
        "user" => match kind {
            "message" => {
                let content: Vec<ContentBlock> =
                    serde_json::from_value(value["content"].clone()).unwrap_or_default();
                vec![Event::UserMessage {
                    id: get_id!(),
                    content,
                }]
            }
            _ => vec![],
        },
        "model" => match kind {
            "message" => {
                let content: Vec<ContentBlock> =
                    serde_json::from_value(value["content"].clone()).unwrap_or_default();
                vec![Event::ModelMessage {
                    id: get_id!(),
                    content,
                }]
            }
            "thinking" => {
                let text = value["text"].as_str().unwrap_or_default().to_string();
                vec![Event::ModelThought {
                    id: get_id!(),
                    text,
                }]
            }
            _ => vec![],
        },
        "function" => match kind {
            "function_call" => {
                let call_id = value
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| value["id"].as_str())
                    .unwrap_or_default()
                    .to_string();
                let args = value["args"].clone();
                vec![Event::FunctionCall {
                    id: get_id!(),
                    call_id,
                    name: name.unwrap_or_default().to_string(),
                    args,
                }]
            }
            "function_result" => {
                let call_id = value
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .or_else(|| value["id"].as_str())
                    .unwrap_or_default()
                    .to_string();
                let content: Option<Vec<ContentBlock>> =
                    serde_json::from_value(value["content"].clone())
                        .ok()
                        .flatten();
                let code = value["code"].as_i64().map(|c| c as i32);
                vec![Event::FunctionResult {
                    id: get_id!(),
                    call_id,
                    name: name.unwrap_or_default().to_string(),
                    content,
                    code,
                }]
            }
            _ => vec![],
        },
        "turn" => match kind {
            "stop_reason" => {
                let sr: StopReason = serde_json::from_value(value["stop_reason"].clone())
                    .unwrap_or(StopReason::EndTurn);
                let token_usage = value
                    .get("token_usage")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());
                vec![Event::TurnStop {
                    id: get_id!(),
                    stop_reason: sr,
                    token_usage,
                }]
            }
            _ => vec![],
        },
        "state" => match kind {
            "state_update" => vec![Event::StateUpdate {
                id: event_id.to_string(),
                name: name.unwrap_or_default().to_string(),
                data: value,
            }],
            _ => vec![],
        },
        _ => vec![],
    }
}

/// 解压并反序列化 state data。
pub(crate) fn decode_state_data<T>(data: &[u8]) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let decompressed = zstd::decode_all(std::io::Cursor::new(data))?;
    Ok(serde_json::from_slice(&decompressed)?)
}

#[cfg(test)]
#[path = "convert_test.rs"]
mod tests;
