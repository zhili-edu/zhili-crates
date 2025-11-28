use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("CREATE SCHEMA kufang").await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE kufang.files (
                id uuid PRIMARY KEY DEFAULT uuidv7(),
                s3_key text NOT NULL,

                size int8 NOT NULL,
                md5 bytea NOT NULL,
                mime text NOT NULL,

                ref_count int4 NOT NULL,
                public bool NOT NULL
            )
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP TABLE kufang.files").await?;

        db.execute_unprepared("DROP SCHEMA kufang").await?;

        Ok(())
    }
}
