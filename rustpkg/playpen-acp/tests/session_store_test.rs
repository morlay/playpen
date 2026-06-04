//! Session 持久化测试（FileBackedSessionStore 单元）

use std::sync::Arc;

use playpen_agent_core::model::Model;
use playpen_agent_core::session::store::SessionManager;
use playpen_acp::session_store::FileBackedSessionStore;

fn test_model() -> Model {
    Model {
        id: "deepseek-v4-pro".into(),
        name: "DeepSeek V4 Pro".into(),
        reasoning_efforts: vec!["low".into(), "high".into()],
        input: vec!["text".into()],
        context_window: 1_000_000,
        max_tokens: 384_000,
        cost: playpen_agent_core::model::Cost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
        },
    }
}

#[test]
fn test_persist_and_load() {
    let temp = tempfile::tempdir().expect("创建临时目录");
    let manager = Arc::new(SessionManager::new());
    let store =
        FileBackedSessionStore::new(temp.path().to_path_buf(), manager.clone()).expect("创建 store");

    let project_root = temp.path().to_string_lossy().to_string();
    let id = store.create(
        "持久化测试",
        test_model(),
        &project_root,
        "default",
        "system prompt",
        vec![],
        None,
    );
    assert!(!id.is_empty());

    let loaded = manager.get(&id).expect("加载");
    assert_eq!(loaded.title, "持久化测试");

    let file = temp.path().join(format!("{}.json", id));
    assert!(file.exists(), "持久化文件应存在: {}", file.display());
}

#[test]
fn test_recovery() {
    let temp = tempfile::tempdir().expect("创建临时目录");
    let store_dir = temp.path().to_path_buf();
    let project_root = temp.path().to_string_lossy().to_string();
    let id: String;

    {
        let manager = Arc::new(SessionManager::new());
        let store = FileBackedSessionStore::new(store_dir.clone(), manager).expect("创建 store");
        id = store.create(
            "恢复测试",
            test_model(),
            &project_root,
            "default",
            "system prompt",
            vec![],
            None,
        );
    }

    {
        let manager = Arc::new(SessionManager::new());
        let _store = FileBackedSessionStore::new(store_dir, manager.clone()).expect("第二次创建 store");
        let loaded = manager.get(&id).expect("恢复后加载");
        assert_eq!(loaded.title, "恢复测试");
    }
}
