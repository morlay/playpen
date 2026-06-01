use super::*;
use crate::service::SessionService;
use futures::StreamExt;
use playpen_content::{ContentBlock, StopReason};

async fn new_service() -> DBSessionService {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("内存数据库连接失败");
    let svc = DBSessionService::new(db);
    svc.migrate().await.expect("迁移失败");
    svc
}

fn init_events_len() -> usize {
    0
}

#[tokio::test]
async fn test_create_and_get() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    assert!(!session.id().is_empty());
    assert_eq!(session.events().len().await, init_events_len());
    assert!(session.state().entities().await.next().await.is_none());

    let loaded = svc.get(session.id()).await.unwrap();
    assert_eq!(loaded.id(), session.id());
}

#[tokio::test]
async fn test_get_nonexistent() {
    let svc = new_service().await;
    let result = svc.get("no-such-session").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_append_event() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    let event = Event::UserMessage {
        id: "msg-1".into(),
        content: vec![ContentBlock::text("hello")],
    };
    let stored = session.events().append(&event).await.unwrap();
    assert!(stored.event_id().is_some());

    let loaded = svc.get(&sid).await.unwrap();
    assert_eq!(loaded.events().len().await, init_events_len() + 1);
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    match &events[0] {
        Event::UserMessage { content, .. } => {
            assert!(!content.is_empty());
        }
        _ => panic!("期望 UserMessage"),
    }
}

#[tokio::test]
async fn test_delta_skipped() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    let delta = Event::ModelMessageDelta {
        id: String::new(),
        text: "partial".into(),
    };
    let stored = session.events().append(&delta).await.unwrap();
    // Delta 不入库，原样返回
    match stored {
        Event::ModelMessageDelta { text, .. } => assert_eq!(text, "partial"),
        _ => panic!("期望原样返回 delta"),
    }

    let loaded = svc.get(&sid).await.unwrap();
    assert_eq!(
        loaded.events().len().await,
        init_events_len(),
        "Delta 不入库"
    );
}

#[tokio::test]
async fn test_state_update_events() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    use serde_json::json;

    // 首次设置两个 key
    let stored = session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: "name".into(),
            data: json!("test-session"),
        })
        .await
        .unwrap();
    assert!(stored.event_id().is_some(), "应返回带 id 的事件");

    session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: "model".into(),
            data: json!("deepseek-v4"),
        })
        .await
        .unwrap();

    let loaded = svc.get(&sid).await.unwrap();
    assert_eq!(
        loaded.state().get("name").await,
        Some(json!("test-session"))
    );
    assert_eq!(
        loaded.state().get("model").await,
        Some(json!("deepseek-v4"))
    );

    // 更新 name
    session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: "name".into(),
            data: json!("renamed"),
        })
        .await
        .unwrap();

    let loaded2 = svc.get(&sid).await.unwrap();
    assert_eq!(loaded2.state().get("name").await, Some(json!("renamed")),);
    assert_eq!(
        loaded2.state().get("model").await,
        Some(json!("deepseek-v4")),
    );
}

#[tokio::test]
async fn test_rewind() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    let event1 = Event::UserMessage {
        id: "msg-1".into(),
        content: vec![ContentBlock::text("first")],
    };
    let eid1 = session.events().append(&event1).await.unwrap();

    let event2 = Event::ModelMessage {
        id: "msg-2".into(),
        content: vec![ContentBlock::text("second")],
    };
    session.events().append(&event2).await.unwrap();

    assert_eq!(
        svc.get(&sid).await.unwrap().events().len().await,
        init_events_len() + 2
    );

    let rewound = svc.rewind(eid1.event_id().unwrap()).await.unwrap();
    assert_eq!(rewound.events().len().await, init_events_len());

    let loaded = svc.get(&sid).await.unwrap();
    assert_eq!(loaded.events().len().await, init_events_len());
}

#[tokio::test]
async fn test_delete() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    svc.delete(&sid).await.unwrap();
    let result = svc.get(&sid).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_stored_event_kinds() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    let events = vec![
        Event::UserMessage {
            id: "u1".into(),
            content: vec![ContentBlock::text("hi")],
        },
        Event::ModelMessage {
            id: "m1".into(),
            content: vec![ContentBlock::text("hello")],
        },
        Event::ModelThought {
            id: "t1".into(),
            text: "reasoning".into(),
        },
        Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "bash".into(),
            args: serde_json::json!({"command": "echo hi"}),
        },
        Event::FunctionResult {
            id: String::new(),
            call_id: "c1".into(),
            name: "bash".into(),
            content: Some(vec![ContentBlock::text("hi")]),
            code: Some(0),
        },
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
    ];

    for ev in &events {
        session.events().append(ev).await.unwrap();
    }

    let loaded = svc.get(&sid).await.unwrap();
    assert_eq!(
        loaded.events().len().await,
        init_events_len() + events.len()
    );

    let events: Vec<Event> = loaded.events().all().await.collect().await;
    match &events[3] {
        Event::FunctionCall { name, .. } => {
            assert_eq!(name, "bash");
        }
        _ => panic!("期望 FunctionCall"),
    }
    match &events[5] {
        Event::TurnStop { .. } => {}
        _ => panic!("期望 TurnStop"),
    }
}

#[tokio::test]
async fn test_token_usage_in_turn_stop() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    let usage = playpen_content::TokenUsage {
        prompt_token_count: 10,
        candidates_token_count: 20,
        total_token_count: 30,
        cache_read_input_token_count: Some(5),
        cache_creation_input_token_count: Some(3),
        thinking_token_count: Some(15),
    };

    session
        .events()
        .append(&Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: Some(usage.clone()),
        })
        .await
        .unwrap();

    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;

    assert_eq!(events.len(), 1);

    let turn_stop = &events[0];
    match turn_stop {
        Event::TurnStop {
            stop_reason,
            token_usage,
            ..
        } => {
            assert_eq!(*stop_reason, StopReason::EndTurn);
            assert!(token_usage.is_some());
            assert_eq!(token_usage.as_ref().unwrap().total_token_count, 30);
            assert_eq!(token_usage.as_ref().unwrap().thinking_token_count, Some(15));
            assert_eq!(
                token_usage.as_ref().unwrap().cache_read_input_token_count,
                Some(5)
            );
            assert_eq!(
                token_usage
                    .as_ref()
                    .unwrap()
                    .cache_creation_input_token_count,
                Some(3)
            );
        }
        _ => panic!("期望 TurnStop"),
    }
}

#[tokio::test]
async fn test_event_id_preserved() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // FunctionCall 的 id（call_id）应被保留
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "call_abc".into(),
            name: "test_tool".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();

    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::FunctionCall { call_id, name, .. } => {
            assert_eq!(call_id, "call_abc", "FunctionCall.id 应被保留");
            assert_eq!(name, "test_tool");
        }
        _ => panic!("期望 FunctionCall"),
    }

    // FunctionResult 的 id 也应被保留
    session
        .events()
        .append(&Event::FunctionResult {
            id: String::new(),
            call_id: "call_abc".into(),
            name: "test_tool".into(),
            content: Some(vec![ContentBlock::text("ok")]),
            code: Some(0),
        })
        .await
        .unwrap();

    let loaded2 = svc.get(&sid).await.unwrap();
    let events2: Vec<Event> = loaded2.events().all().await.collect().await;
    assert_eq!(events2.len(), 2);
    match &events2[1] {
        Event::FunctionResult {
            call_id,
            name,
            content,
            code,
            ..
        } => {
            assert_eq!(call_id, "call_abc", "FunctionResult.id 应被保留");
            assert_eq!(name, "test_tool");
            assert!(content.is_some());
            assert_eq!(*code, Some(0));
        }
        _ => panic!("期望 FunctionResult"),
    }
}

#[tokio::test]
async fn test_model_message_id_preserved() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    // ModelMessage 的 id 应被保留（如果设置了）
    session
        .events()
        .append(&Event::ModelMessage {
            id: "ext-id-1".into(),
            content: vec![ContentBlock::text("hello")],
        })
        .await
        .unwrap();

    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ModelMessage { id, content } => {
            assert_eq!(id.as_str(), "ext-id-1", "ModelMessage.id 应被保留");
            assert!(!content.is_empty());
        }
        _ => panic!("期望 ModelMessage"),
    }
}

#[tokio::test]
async fn test_model_thought_id_preserved() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let sid = session.id().to_string();

    session
        .events()
        .append(&Event::ModelThought {
            id: "thought-1".into(),
            text: "reasoning...".into(),
        })
        .await
        .unwrap();

    let loaded = svc.get(&sid).await.unwrap();
    let events: Vec<Event> = loaded.events().all().await.collect().await;
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::ModelThought { id, text } => {
            assert_eq!(id.as_str(), "thought-1", "ModelThought.id 应被保留");
            assert_eq!(text, "reasoning...");
        }
        _ => panic!("期望 ModelThought"),
    }
}

// ── State ──

async fn set_state(session: &dyn Session, key: &str, value: serde_json::Value) {
    session
        .events()
        .append(&Event::StateUpdate {
            id: String::new(),
            name: key.into(),
            data: value,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn test_state_get() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    set_state(&*session, "key1", serde_json::json!("val1")).await;
    set_state(&*session, "key2", serde_json::json!(42)).await;

    assert_eq!(
        session.state().get("key1").await,
        Some(serde_json::json!("val1"))
    );
    assert_eq!(
        session.state().get("key2").await,
        Some(serde_json::json!(42))
    );
    assert_eq!(session.state().get("nonexistent").await, None);
}

#[tokio::test]
async fn test_state_get_latest() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    set_state(&*session, "k", serde_json::json!("old")).await;
    set_state(&*session, "k", serde_json::json!("new")).await;

    assert_eq!(
        session.state().get("k").await,
        Some(serde_json::json!("new")),
        "get 应返回最新值"
    );
}

#[tokio::test]
async fn test_state_entities() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    set_state(&*session, "a", serde_json::json!(1)).await;
    set_state(&*session, "b", serde_json::json!(2)).await;
    set_state(&*session, "a", serde_json::json!(10)).await;

    let mut entities: Vec<(String, serde_json::Value)> =
        session.state().entities().await.collect().await;
    entities.sort_by(|a, b| a.0.cmp(&b.0));
    // GROUP BY name 只返回每个 key 一行
    assert_eq!(entities.len(), 2, "应返回 2 个唯一 key");
    assert_eq!(entities[0].0, "a");
    assert_eq!(entities[1].0, "b");
}

// ── Events by_role ──

#[tokio::test]
async fn test_events_by_role() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("hi")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("hello")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "bash".into(),
            args: serde_json::json!({}),
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        })
        .await
        .unwrap();

    let user_events: Vec<Event> = session
        .events()
        .by_role(&[Role::User])
        .all()
        .await
        .collect()
        .await;
    assert_eq!(user_events.len(), 1);
    assert!(matches!(user_events[0], Event::UserMessage { .. }));

    let model_events: Vec<Event> = session
        .events()
        .by_role(&[Role::Model])
        .all()
        .await
        .collect()
        .await;
    assert_eq!(model_events.len(), 1);

    let function_events: Vec<Event> = session
        .events()
        .by_role(&[Role::Function])
        .all()
        .await
        .collect()
        .await;
    assert_eq!(function_events.len(), 1);

    let turn_events: Vec<Event> = session
        .events()
        .by_role(&[Role::Turn])
        .all()
        .await
        .collect()
        .await;
    assert_eq!(turn_events.len(), 1);
}

#[tokio::test]
async fn test_events_all_sequencing() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    let e1 = session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("1")],
        })
        .await
        .unwrap();
    assert!(e1.event_id().is_some(), "append_event 应返回带 id 的事件");

    let e2 = session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("2")],
        })
        .await
        .unwrap();

    let events: Vec<Event> = session.events().all().await.collect().await;
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].event_id(),
        e1.event_id(),
        "时序错误：事件 1 应排在首位"
    );
    assert_eq!(
        events[1].event_id(),
        e2.event_id(),
        "时序错误：事件 2 应排在第二位"
    );
}

// ── SessionService list ──

#[tokio::test]
async fn test_list_sessions() {
    let svc = new_service().await;
    assert_eq!(svc.list(None, 0).await.unwrap().len(), 0);

    let _s1 = svc.create().await.unwrap();
    let _s2 = svc.create().await.unwrap();

    let all = svc.list(None, 0).await.unwrap();
    assert_eq!(all.len(), 2);

    let limited = svc.list(Some(1), 0).await.unwrap();
    assert_eq!(limited.len(), 1);
}

#[tokio::test]
async fn test_rewind_first_event() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();
    let _sid = session.id().to_string();

    let e1 = session
        .events()
        .append(&Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("first")],
        })
        .await
        .unwrap();
    session
        .events()
        .append(&Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("second")],
        })
        .await
        .unwrap();

    let rewound = svc.rewind(e1.event_id().unwrap()).await.unwrap();
    assert_eq!(rewound.events().len().await, 0);
}

#[tokio::test]
async fn test_append_all_event_variants() {
    let svc = new_service().await;
    let session = svc.create().await.unwrap();

    let variants: Vec<Event> = vec![
        Event::UserMessage {
            id: String::new(),
            content: vec![ContentBlock::text("u")],
        },
        Event::ModelMessage {
            id: String::new(),
            content: vec![ContentBlock::text("m")],
        },
        Event::ModelThought {
            id: String::new(),
            text: "t".into(),
        },
        Event::FunctionCall {
            id: String::new(),
            call_id: "c1".into(),
            name: "bash".into(),
            args: serde_json::json!({}),
        },
        Event::FunctionResult {
            id: String::new(),
            call_id: "c1".into(),
            name: "bash".into(),
            content: None,
            code: None,
        },
        Event::TurnStop {
            id: String::new(),
            stop_reason: StopReason::EndTurn,
            token_usage: None,
        },
        Event::StateUpdate {
            id: String::new(),
            name: "k".into(),
            data: serde_json::json!("v"),
        },
    ];

    for ev in &variants {
        let stored = session.events().append(ev).await.unwrap();
        if !matches!(
            ev,
            Event::ModelMessageDelta { .. } | Event::ModelThoughtDelta { .. }
        ) {
            assert!(stored.event_id().is_some(), "{:?} 应返回带 id 的事件", ev);
        }
    }

    assert_eq!(session.events().len().await, variants.len());
}
