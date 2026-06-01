/// 事件角色分类。对应 DB `events.role` 列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Model,
    Function,
    Turn,
    State,
}

impl Role {
    /// 返回 DB 中存储的字符串值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Model => "model",
            Role::Function => "function",
            Role::Turn => "turn",
            Role::State => "state",
        }
    }
}
