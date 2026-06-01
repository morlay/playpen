use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_session_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub f_id: i64,
    #[sea_orm(indexed)]
    pub f_session_id: String,
    #[sea_orm(indexed)]
    pub f_event_id: String,
    pub f_sequence: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
