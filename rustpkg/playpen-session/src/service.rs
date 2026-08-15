use async_trait::async_trait;

use crate::session::Session;

#[async_trait]
pub trait SessionService: Send + Sync {
    /// 创建新 session。
    async fn create(&self) -> anyhow::Result<Box<dyn Session>>;

    /// 获取 session。
    async fn get(&self, id: &str) -> anyhow::Result<Box<dyn Session>>;

    /// 删除指定事件及其后所有事件（含指定事件本身），session 头指针回退到指定事件的前一事件。
    async fn rewind(&self, event_id: &str) -> anyhow::Result<Box<dyn Session>>;

    /// 删除 session。
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// 列出 session。
    async fn list(
        &self,
        limit: Option<usize>,
        offset: usize,
    ) -> anyhow::Result<Vec<Box<dyn Session>>>;
}
