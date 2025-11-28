use sea_orm_migration::prelude::*;

mod m0001_create_table;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migration_table_name() -> DynIden {
        "_seaql_migrations_kufang".into()
    }

    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m0001_create_table::Migration)]
    }
}
