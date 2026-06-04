use super::*;

fn test_memory() -> SqliteMemory {
    let conn = std::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap());
    conn.lock().unwrap().execute_batch(
        "CREATE TABLE IF NOT EXISTS messages (
            conversation_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            content TEXT NOT NULL,
            PRIMARY KEY (conversation_id, seq)
        )"
    ).unwrap();
    SqliteMemory { conn: Arc::new(conn) }
}

#[test]
fn memory_load_empty() {
    let mem = test_memory();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let msgs = rt.block_on(mem.load("test")).unwrap();
    assert!(msgs.is_empty());
}

#[test]
fn memory_append_and_load() {
    let mem = test_memory();
    let msg = rig_core::completion::Message::user("hello");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(mem.append("test", vec![msg.clone()])).unwrap();
    let loaded = rt.block_on(mem.load("test")).unwrap();
    assert_eq!(loaded.len(), 1);
}

#[test]
fn memory_clear() {
    let mem = test_memory();
    let msg = rig_core::completion::Message::user("hello");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(mem.append("test", vec![msg])).unwrap();
    rt.block_on(mem.clear("test")).unwrap();
    let loaded = rt.block_on(mem.load("test")).unwrap();
    assert!(loaded.is_empty());
}
