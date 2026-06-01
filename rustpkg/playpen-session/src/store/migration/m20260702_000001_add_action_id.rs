use sea_orm_migration::prelude::*;

/// Add f_action_id column to t_events for role=function call_id indexing
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Tables::TEvents)
                    .add_column_if_not_exists(
                        ColumnDef::new(ActionId::FActionId)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_events_action_id")
                    .table(Tables::TEvents)
                    .col(ActionId::FActionId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

enum Tables {
    TEvents,
}

impl Iden for Tables {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        match self {
            Tables::TEvents => write!(s, "t_events").unwrap(),
        }
    }
}

enum ActionId {
    FActionId,
}

impl Iden for ActionId {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        match self {
            ActionId::FActionId => write!(s, "f_action_id").unwrap(),
        }
    }
}
