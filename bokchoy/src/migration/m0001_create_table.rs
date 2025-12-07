use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("CREATE SCHEMA bokchoy").await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE bokchoy.payments (
                id uuid PRIMARY KEY DEFAULT uuidv7(),
                provider_trade_no text UNIQUE,

                description text NOT NULL,
                status int2 NOT NULL,
                amount int8 NOT NULL,
                refunded_amount int8 NOT NULL DEFAULT 0,

                biz_id uuid NOT NULL,

                provider int2 NOT NULL,
                provider_info jsonb NOT NULL DEFAULT '{}',

                created_at timestamptz NOT NULL DEFAULT now(),
                updated_at timestamptz NOT NULL DEFAULT now(),
                success_at timestamptz
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE bokchoy.payment_events (
                id uuid PRIMARY KEY DEFAULT uuidv7(),
                payment_id uuid REFERENCES bokchoy.payments NOT NULL,

                kind int2 NOT NULL,
                http_req jsonb NOT NULL,
                http_res jsonb,

                created_at timestamptz NOT NULL DEFAULT now()
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE bokchoy.refunds (
                id uuid PRIMARY KEY DEFAULT uuidv7(),
                payment_id uuid REFERENCES bokchoy.payments NOT NULL,
                provider_refund_no text,

                amount int8 NOT NULL,
                reason text,
                status int2 NOT NULL,

                created_at timestamptz NOT NULL DEFAULT now(),
                updated_at timestamptz NOT NULL DEFAULT now(),
                success_at timestamptz
            )
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP TABLE bokchoy.refunds").await?;

        db.execute_unprepared("DROP TABLE bokchoy.payment_events")
            .await?;

        db.execute_unprepared("DROP TABLE bokchoy.payments").await?;

        db.execute_unprepared("DROP SCHEMA bokchoy").await?;

        Ok(())
    }
}
