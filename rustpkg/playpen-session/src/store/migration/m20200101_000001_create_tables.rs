use sea_orm_migration::prelude::*;

/// Initial tables: t_sessions, t_events, t_session_events
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Tables::TSessions)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SessionsCol::FId)
                            .auto_increment()
                            .big_integer()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SessionsCol::FSessionId).string().not_null())
                    .col(
                        ColumnDef::new(SessionsCol::FHeadEventId)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(SessionsCol::FHeadSequence)
                            .integer()
                            .not_null()
                            .default(-1),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_sessions_session_id")
                    .table(Tables::TSessions)
                    .col(SessionsCol::FSessionId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Tables::TEvents)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EventsCol::FId)
                            .auto_increment()
                            .big_integer()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(EventsCol::FEventId).string().not_null())
                    .col(
                        ColumnDef::new(EventsCol::FParentId)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FKind)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FRole)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FName)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FEncoding)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FData)
                            .blob()
                            .not_null()
                            .default(Vec::<u8>::new()),
                    )
                    .col(
                        ColumnDef::new(EventsCol::FCreatedAt)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        for (name, col) in [
            ("idx_events_event_id", EventsCol::FEventId),
            ("idx_events_parent_id", EventsCol::FParentId),
            ("idx_events_kind", EventsCol::FKind),
            ("idx_events_role", EventsCol::FRole),
            ("idx_events_created_at", EventsCol::FCreatedAt),
        ] {
            manager
                .create_index(
                    Index::create()
                        .if_not_exists()
                        .name(name)
                        .table(Tables::TEvents)
                        .col(col)
                        .to_owned(),
                )
                .await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(Tables::TSessionEvents)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SessionEventsCol::FId)
                            .auto_increment()
                            .big_integer()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SessionEventsCol::FSessionId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SessionEventsCol::FEventId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SessionEventsCol::FSequence)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_session_events_chain")
                    .table(Tables::TSessionEvents)
                    .col(SessionEventsCol::FSessionId)
                    .col(SessionEventsCol::FSequence)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[allow(clippy::enum_variant_names)]
enum Tables {
    TEvents,
    TSessionEvents,
    TSessions,
}

impl Iden for Tables {
    fn unquoted(&self) -> &str {
        match self {
            Tables::TEvents => "t_events",
            Tables::TSessionEvents => "t_session_events",
            Tables::TSessions => "t_sessions",
        }
    }
}

#[allow(clippy::enum_variant_names)]
enum EventsCol {
    FId,
    FEventId,
    FParentId,
    FKind,
    FRole,
    FName,
    FEncoding,
    FData,
    FCreatedAt,
}

impl Iden for EventsCol {
    fn unquoted(&self) -> &str {
        match self {
            EventsCol::FId => "f_id",
            EventsCol::FEventId => "f_event_id",
            EventsCol::FParentId => "f_parent_id",
            EventsCol::FKind => "f_kind",
            EventsCol::FRole => "f_role",
            EventsCol::FName => "f_name",
            EventsCol::FEncoding => "f_encoding",
            EventsCol::FData => "f_data",
            EventsCol::FCreatedAt => "f_created_at",
        }
    }
}

#[allow(clippy::enum_variant_names)]
enum SessionEventsCol {
    FId,
    FSessionId,
    FEventId,
    FSequence,
}

impl Iden for SessionEventsCol {
    fn unquoted(&self) -> &str {
        match self {
            SessionEventsCol::FId => "f_id",
            SessionEventsCol::FSessionId => "f_session_id",
            SessionEventsCol::FEventId => "f_event_id",
            SessionEventsCol::FSequence => "f_sequence",
        }
    }
}

#[allow(clippy::enum_variant_names)]
enum SessionsCol {
    FId,
    FSessionId,
    FHeadEventId,
    FHeadSequence,
}

impl Iden for SessionsCol {
    fn unquoted(&self) -> &str {
        match self {
            SessionsCol::FId => "f_id",
            SessionsCol::FSessionId => "f_session_id",
            SessionsCol::FHeadEventId => "f_head_event_id",
            SessionsCol::FHeadSequence => "f_head_sequence",
        }
    }
}
