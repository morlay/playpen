use async_trait::async_trait;
use futures::stream::BoxStream;
use serde_json::Value;

/// Session 状态存储：key-value，value 为 JSON。
/// 异步访问，由具体实现决定查询方式。
#[async_trait]
pub trait State: Send + Sync {
    async fn get(&self, key: &str) -> Option<Value>;
    async fn set(&mut self, key: String, value: Value);
    /// 以流的形式遍历所有状态条目。
    /// 每次 yield (key, value)。
    async fn entities(&self) -> BoxStream<'_, (String, Value)>;
}
