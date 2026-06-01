use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_sessions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub f_id: i64,
    /// 业务 ID（UUID v7）
    #[sea_orm(indexed)]
    pub f_session_id: String,
    /// 空字符串表示无 head（初始状态或已回退清空）
    pub f_head_event_id: String,
    pub f_head_sequence: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
