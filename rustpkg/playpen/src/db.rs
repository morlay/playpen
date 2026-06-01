/// 返回 sessions.db 路径，位于 XDG_CACHE_HOME/playpen/ 下。
pub(crate) fn sessions_db_path() -> std::path::PathBuf {
    let cache_dir = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            std::path::PathBuf::from(home).join(".cache")
        });
    let dir = cache_dir.join("playpen");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("sessions.db")
}
