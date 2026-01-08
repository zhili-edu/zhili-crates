use crate::{Order, OrderItem, OrderQuery, OrderStatus};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgConnection, QueryBuilder};
use uuid::Uuid;

pub mod mock;

pub use mock::MockOrderRepository;

pub struct OrderCreateArgs {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,
    pub items: Vec<OrderItemCreateArgs>,
    pub payment_fee: i64,
    pub discount_amount: i64,
    pub total_items_amount: i64,
    pub payable_amount: i64,
    pub extra_info: Option<Value>,
}

pub struct OrderItemCreateArgs {
    pub id: Uuid,
    pub order_id: Uuid,
    pub sku_id: Uuid,
    pub sku_type: String,
    pub original_price: i64,
    pub unit_price: i64,
    pub real_amount: i64,
    pub extra_info: Option<Value>,
}

pub struct OrderPaymentUpdateArgs {
    pub order_id: Uuid,
    pub payment_amount: i64,
    pub new_paid_amount: i64,
    pub new_status: OrderStatus,
}

pub struct OrderRefundUpdateArgs {
    pub order_id: Uuid,
    pub refund_amount: i64,
    pub new_refunded_amount: i64,
    pub new_status: OrderStatus,
}

#[async_trait]
pub trait OrderRepository: Send + Sync + std::fmt::Debug {
    type Context;

    async fn query_orders(
        &self,
        conn: &mut Self::Context,
        query: &crate::OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error>;

    async fn query_order_optional(
        &self,
        conn: &mut Self::Context,
        query: &crate::OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error>;

    async fn get_orders_items(
        &self,
        conn: &mut Self::Context,
        order_ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error>;

    async fn get_items_by_ids(
        &self,
        conn: &mut Self::Context,
        ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error>;

    async fn create(
        &self,
        conn: &mut Self::Context,
        args: OrderCreateArgs,
    ) -> Result<Uuid, sqlx::Error>;

    async fn update_status(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), sqlx::Error>;

    async fn update_payment(
        &self,
        conn: &mut Self::Context,
        args: OrderPaymentUpdateArgs,
    ) -> Result<(), sqlx::Error>;

    async fn update_refund(
        &self,
        conn: &mut Self::Context,
        args: OrderRefundUpdateArgs,
    ) -> Result<(), sqlx::Error>;

    async fn update_extra_info(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error>;

    async fn update_item_extra_info(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error>;

    async fn cancel_expired_orders(&self, conn: &mut Self::Context) -> Result<u64, sqlx::Error>;
}

#[derive(Debug)]
pub struct OrderRepo;

#[async_trait]
impl OrderRepository for OrderRepo {
    type Context = PgConnection;

    async fn query_orders(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        let mut builder = QueryBuilder::new(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount, 
                payable_amount, paid_amount, refunded_amount, refund_fee,
                created_at, updated_at, expire_at, extra_info
            FROM jidan.orders
            WHERE 1=1
            "#,
        );

        query.apply_filters(&mut builder);

        builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(limit);
        }
        if query.offset > 0 {
            builder.push(" OFFSET ");
            builder.push_bind(query.offset);
        }

        builder
            .build_query_as::<Order>()
            .fetch_all(&mut *conn)
            .await
    }

    async fn query_order_optional(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        let mut builder = QueryBuilder::new(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount, 
                payable_amount, paid_amount, refunded_amount, refund_fee,
                created_at, updated_at, expire_at, extra_info
            FROM jidan.orders
            WHERE 1=1
            "#,
        );

        query.apply_filters(&mut builder);

        builder.push(" ORDER BY created_at DESC");

        builder.push(" LIMIT 1");

        builder
            .build_query_as::<Order>()
            .fetch_optional(&mut *conn)
            .await
    }

    async fn get_orders_items(
        &self,
        conn: &mut Self::Context,
        order_ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        sqlx::query_as::<_, OrderItem>(
            r#"
            SELECT
                id, sku_id, sku_type, order_id,
                original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE order_id = ANY($1)
            "#,
        )
        .bind(order_ids)
        .fetch_all(&mut *conn)
        .await
    }

    async fn get_items_by_ids(
        &self,
        conn: &mut Self::Context,
        ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        sqlx::query_as::<_, OrderItem>(
            r#"
            SELECT
                id, sku_id, sku_type, order_id,
                original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_all(&mut *conn)
        .await
    }

    async fn create(
        &self,
        conn: &mut Self::Context,
        args: OrderCreateArgs,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO jidan.orders (
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount,
                payable_amount,
                extra_info
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9,
                $10
            )
            "#,
        )
        .bind(args.id)
        .bind(args.user_id)
        .bind(args.channel)
        .bind(args.channel_no)
        .bind(OrderStatus::Pending)
        .bind(args.total_items_amount)
        .bind(args.payment_fee)
        .bind(args.discount_amount)
        .bind(args.payable_amount)
        .bind(
            args.extra_info
                .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
        )
        .execute(&mut *conn)
        .await?;

        let sku_type: Vec<String> = args.items.iter().map(|i| i.sku_type.clone()).collect();
        let sku_id: Vec<Uuid> = args.items.iter().map(|i| i.sku_id).collect();
        let original_price: Vec<i64> = args.items.iter().map(|i| i.original_price).collect();
        let unit_price: Vec<i64> = args.items.iter().map(|i| i.unit_price).collect();
        let real_amount: Vec<i64> = args.items.iter().map(|i| i.real_amount).collect();
        let extra_info: Vec<Option<Value>> =
            args.items.iter().map(|i| i.extra_info.clone()).collect();

        sqlx::query(
            r#"
            WITH new_items AS (
                SELECT *
                FROM UNNEST($2, $3, $4, $5, $6, $7)
                    AS t (sku_id, sku_type, original_price, unit_price, real_amount, extra_info)
            )
            INSERT INTO jidan.order_items (
                order_id, sku_id, sku_type,
                original_price, unit_price, real_amount, extra_info
            )
            SELECT
                $1 AS order_id,
                sku_id, sku_type, original_price, unit_price, real_amount, extra_info
            FROM new_items
            "#,
        )
        .bind(args.id)
        .bind(sku_id)
        .bind(sku_type)
        .bind(original_price)
        .bind(unit_price)
        .bind(real_amount)
        .bind(
            extra_info
                .into_iter()
                .map(|info| info.unwrap_or_else(|| serde_json::Value::Object(Default::default())))
                .collect::<Vec<_>>(),
        )
        .execute(&mut *conn)
        .await?;

        Ok(args.id)
    }

    async fn update_status(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(status)
        .bind(order_id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    async fn update_payment(
        &self,
        conn: &mut Self::Context,
        args: OrderPaymentUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, paid_amount = $2, updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(args.new_status)
        .bind(args.new_paid_amount)
        .bind(args.order_id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    async fn update_refund(
        &self,
        conn: &mut Self::Context,
        args: OrderRefundUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, refunded_amount = $2, updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(args.new_status)
        .bind(args.new_refunded_amount)
        .bind(args.order_id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    async fn update_extra_info(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET updated_at = now(), extra_info = extra_info || $1
            WHERE id = $2
            "#,
        )
        .bind(extra_info)
        .bind(order_id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    async fn update_item_extra_info(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.order_items
            SET extra_info = extra_info || $1
            WHERE id = $2
            "#,
        )
        .bind(extra_info)
        .bind(id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    async fn cancel_expired_orders(&self, conn: &mut Self::Context) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, updated_at = now()
            WHERE
                status = $2
                AND expire_at IS NOT NULL
                AND expire_at < now()
            "#,
        )
        .bind(OrderStatus::Canceled)
        .bind(OrderStatus::Pending)
        .execute(&mut *conn)
        .await?;

        Ok(result.rows_affected())
    }
}
