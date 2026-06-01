use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub f_id: i64,
    /// 业务 ID（UUID v7）
    #[sea_orm(indexed)]
    pub f_event_id: String,
    /// 空字符串表示 root
    #[sea_orm(indexed)]
    pub f_parent_id: String,

    /// message | thinking | function_call | function_result | {stop_reason}
    #[sea_orm(indexed)]
    pub f_kind: String,
    /// user | model | function | turn
    #[sea_orm(indexed)]
    pub f_role: String,
    /// {function_name}  | {state_key}
    pub f_name: String,

    /// role 为 function 时存 call_id，其余为空字符串
    #[sea_orm(indexed)]
    pub f_action_id: String,

    /// event data 编码方式
    pub f_encoding: String,
    /// event data
    pub f_data: Vec<u8>,

    #[sea_orm(indexed)]
    pub f_created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
