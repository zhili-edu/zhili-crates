use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("CREATE SCHEMA jidan").await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE jidan.orders (
                id uuid PRIMARY KEY DEFAULT uuidv7(),

                user_id uuid NOT NULL,
                channel text NOT NULL,
                channel_no text,
                status int2 NOT NULL,

                total_items_amount int8 NOT NULL,
                discount_amount int8 NOT NULL DEFAULT 0,
                payable_amount int8 NOT NULL,

                payment_fee int8 NOT NULL DEFAULT 0,
                paid_amount int8 NOT NULL DEFAULT 0,

                refund_fee int8 NOT NULL DEFAULT 0,
                refunded_amount int8 NOT NULL DEFAULT 0,

                created_at timestamptz NOT NULL DEFAULT now(),
                updated_at timestamptz NOT NULL DEFAULT now(),
                expire_at timestamptz,

                extra_info jsonb
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            CREATE TABLE jidan.order_items (
                id uuid PRIMARY KEY DEFAULT uuidv7(),
                order_id uuid REFERENCES jidan.orders NOT NULL,

                item_id uuid NOT NULL,
                item_type text NOT NULL,

                original_price int8 NOT NULL,
                unit_price int8 NOT NULL,
                real_amount int8 NOT NULL,

                extra_info jsonb
            )
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("DROP TABLE jidan.order_items")
            .await?;

        db.execute_unprepared("DROP TABLE jidan.orders").await?;

        db.execute_unprepared("DROP SCHEMA jidan").await?;

        Ok(())
    }
}
