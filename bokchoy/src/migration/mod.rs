use sea_orm_migration::prelude::*;

mod m0001_create_table;
mod m0002_add_payment_expire_at;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migration_table_name() -> DynIden {
        "_seaql_migrations_bokchoy".into()
    }

    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m0001_create_table::Migration),
            Box::new(m0002_add_payment_expire_at::Migration),
        ]
    }
}
