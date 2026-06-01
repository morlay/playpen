use async_trait::async_trait;
use futures::stream::BoxStream;
use playpen_content::Event;

use crate::Role;

/// Session 事件序列（按追加顺序）。异步访问。
#[async_trait]
pub trait Events: Send + Sync {
    /// 以流的形式遍历所有事件（按追加顺序）。
    async fn all(&self) -> BoxStream<'_, Event>;

    /// 事件序列长度
    async fn len(&self) -> usize;

    /// 事件序列是否为空
    async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// 追加事件到链尾，返回写入后的 Event（含 store 分配的 id）。
    async fn append(&self, event: &Event) -> anyhow::Result<Event>;

    /// 按角色过滤，返回带过滤条件的 Events（builder 模式）。
    fn by_role(&self, roles: &[Role]) -> Box<dyn Events + '_>;
}
