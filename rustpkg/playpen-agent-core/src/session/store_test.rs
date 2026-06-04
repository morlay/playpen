use super::*;
use crate::model::Model;
use crate::session::session::create_session;

fn test_model() -> Model {
    Model {
        id: "test".into(), name: "Test".into(),
        reasoning_efforts: vec!["off".into()], input: vec!["text".into()],
        context_window: 128000, max_tokens: 4096,
        cost: Default::default(),
    }
}

#[test]
fn store_crud() -> anyhow::Result<()> {
    let mgr = SessionManager::new();
    let s = create_session("t".into(), test_model(), "/tmp".into(), "a".into(), "p".into(), vec![], None);
    let id = s.id.clone();
    mgr.insert(s);

    assert!(mgr.get(&id).is_some());
    mgr.archive(&id)?;
    mgr.delete(&id)?;
    assert!(mgr.get(&id).is_none());
    Ok(())
}

#[test]
fn store_fork_preserves_messages() -> anyhow::Result<()> {
    let mgr = SessionManager::new();
    let s = create_session("t".into(), test_model(), "/tmp".into(), "a".into(), "p".into(), vec![], None);
    let id = s.id.clone();
    mgr.insert(s);
    mgr.append_message(&id, Message::user("hello"))?;
    let forked = mgr.fork(&id, "agent")?;
    let fork_session = mgr.get(&forked).unwrap();
    assert_eq!(fork_session.messages.len(), 1);
    assert!(fork_session.title.contains("fork"));
    Ok(())
}
