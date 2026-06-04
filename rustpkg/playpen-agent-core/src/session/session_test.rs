use super::*;

fn test_model() -> Model {
    Model {
        id: "test".into(), name: "Test".into(),
        reasoning_efforts: vec!["off".into()], input: vec!["text".into()],
        context_window: 128000, max_tokens: 4096,
        cost: Default::default(),
    }
}

#[test]
fn context_usage_below_limit() {
    let mut s = create_session("t".into(), test_model(), "/tmp".into(), "a".into(), "p".into(), vec![], Some(10000));
    s.total_tokens = Some(5000);
    assert!((s.context_usage().unwrap() - 0.5).abs() < 0.001);
    assert!(!s.is_context_near_limit());
}

#[test]
fn context_usage_near_limit() {
    let mut s = create_session("t".into(), test_model(), "/tmp".into(), "a".into(), "p".into(), vec![], Some(10000));
    s.total_tokens = Some(9500);
    assert!(s.is_context_near_limit());
}

#[test]
fn context_usage_no_data() {
    let s = create_session("t".into(), test_model(), "/tmp".into(), "a".into(), "p".into(), vec![], None);
    assert_eq!(s.context_usage(), None);
    assert!(!s.is_context_near_limit());
}

#[test]
fn session_has_unique_id() {
    let m = test_model();
    let s1 = create_session("a".into(), m.clone(), "/tmp".into(), "x".into(), "p".into(), vec![], None);
    let s2 = create_session("b".into(), m.clone(), "/tmp".into(), "x".into(), "p".into(), vec![], None);
    assert_ne!(s1.id, s2.id);
}
