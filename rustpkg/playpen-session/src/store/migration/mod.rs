use sea_orm_migration::prelude::*;

mod m20200101_000001_create_tables;
mod m20260702_000001_add_action_id;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20200101_000001_create_tables::Migration),
            Box::new(m20260702_000001_add_action_id::Migration),
        ]
    }
}
