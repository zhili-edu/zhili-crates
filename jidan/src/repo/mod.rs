use crate::{Order, OrderItem, OrderQuery, OrderStatus};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub mod mock;

pub use mock::MockOrderRepository;

pub struct OrderCreateArgs {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,
    pub items: Vec<OrderItemCreateArgs>,
    pub discount_amount: i64,
    pub payable_amount: i64,
    pub channel_fee: i64,
    pub extra_info: Option<Value>,
}

pub struct OrderItemCreateArgs {
    pub id: Uuid,
    pub order_id: Uuid,
    pub sku_id: Uuid,
    pub sku_type: String,
    pub unit_price: i64,
    pub list_price: i64,
    pub discount_amount: i64,
    pub payable_amount: i64,
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

    async fn query(
        &self,
        conn: &mut Self::Context,
        query: &crate::OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error>;

    async fn query_one(
        &self,
        conn: &mut Self::Context,
        query: &crate::OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error>;

    async fn query_one_for_update(
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

    async fn mark_item_refunded(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        item_id: Uuid,
    ) -> Result<(), sqlx::Error>;

    async fn close_expired_orders(&self, conn: &mut Self::Context) -> Result<u64, sqlx::Error>;
}

#[derive(Debug)]
pub struct OrderRepo;

#[async_trait]
impl OrderRepository for OrderRepo {
    type Context = PgConnection;

    async fn query(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        let offset = query.offset.max(0);
        let limit = query.limit.unwrap_or(i64::MAX);

        let has_skus = match query.has_skus {
            Some(sku_ids) if sku_ids.is_empty() => None,
            Some(sku_ids) => Some(sku_ids),
            None => None,
        };

        sqlx::query_as::<_, Order>(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                discount_amount, payable_amount, paid_amount, refunded_amount, channel_fee,
                created_at, updated_at, expire_at, extra_info
            FROM jidan.orders
            WHERE ($1::uuid IS NULL OR id = $1)
              AND ($2::uuid IS NULL OR user_id = $2)
              AND ($3::int2 IS NULL OR status = $3)
              AND ($4::text IS NULL OR channel = $4)
              AND ($5::text IS NULL OR channel_no = $5)
              AND ($6::timestamptz IS NULL OR created_at >= $6)
              AND ($7::timestamptz IS NULL OR created_at < $7)
              AND (
                    $8::uuid[] IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE sku_id = ANY($8)
                    )
                  )
              AND ($9::jsonb IS NULL OR extra_info @> $9)
              AND (
                    $10::jsonb IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE extra_info @> $10
                    )
                  )
            ORDER BY created_at DESC
            LIMIT $11
            OFFSET $12
            "#,
        )
        .bind(query.id)
        .bind(query.user_id)
        .bind(query.status)
        .bind(query.channel.as_deref())
        .bind(query.channel_no.as_deref())
        .bind(query.created_after)
        .bind(query.created_before)
        .bind(has_skus)
        .bind(query.extra_info)
        .bind(query.item_extra_info)
        .bind(limit)
        .bind(offset)
        .fetch_all(&mut *conn)
        .await
    }

    async fn query_one(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        let has_skus = match query.has_skus {
            Some(sku_ids) if sku_ids.is_empty() => None,
            Some(sku_ids) => Some(sku_ids),
            None => None,
        };

        sqlx::query_as::<_, Order>(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                discount_amount, payable_amount, paid_amount, refunded_amount, channel_fee,
                created_at, updated_at, expire_at, extra_info
            FROM jidan.orders
            WHERE ($1::uuid IS NULL OR id = $1)
              AND ($2::uuid IS NULL OR user_id = $2)
              AND ($3::int2 IS NULL OR status = $3)
              AND ($4::text IS NULL OR channel = $4)
              AND ($5::text IS NULL OR channel_no = $5)
              AND ($6::timestamptz IS NULL OR created_at >= $6)
              AND ($7::timestamptz IS NULL OR created_at < $7)
              AND (
                    $8::uuid[] IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE sku_id = ANY($8)
                    )
                  )
              AND ($9::jsonb IS NULL OR extra_info @> $9)
              AND (
                    $10::jsonb IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE extra_info @> $10
                    )
                  )
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(query.id)
        .bind(query.user_id)
        .bind(query.status)
        .bind(query.channel.as_deref())
        .bind(query.channel_no.as_deref())
        .bind(query.created_after)
        .bind(query.created_before)
        .bind(has_skus)
        .bind(query.extra_info)
        .bind(query.item_extra_info)
        .fetch_optional(&mut *conn)
        .await
    }

    async fn query_one_for_update(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        let has_skus = match query.has_skus {
            Some(sku_ids) if sku_ids.is_empty() => None,
            Some(sku_ids) => Some(sku_ids),
            None => None,
        };

        sqlx::query_as::<_, Order>(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                discount_amount, payable_amount, paid_amount, refunded_amount, channel_fee,
                created_at, updated_at, expire_at, extra_info
            FROM jidan.orders
            WHERE ($1::uuid IS NULL OR id = $1)
              AND ($2::uuid IS NULL OR user_id = $2)
              AND ($3::int2 IS NULL OR status = $3)
              AND ($4::text IS NULL OR channel = $4)
              AND ($5::text IS NULL OR channel_no = $5)
              AND ($6::timestamptz IS NULL OR created_at >= $6)
              AND ($7::timestamptz IS NULL OR created_at < $7)
              AND (
                    $8::uuid[] IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE sku_id = ANY($8)
                    )
                  )
              AND ($9::jsonb IS NULL OR extra_info @> $9)
              AND (
                    $10::jsonb IS NULL
                    OR id IN (
                        SELECT order_id
                        FROM jidan.order_items
                        WHERE extra_info @> $10
                    )
                  )
            ORDER BY created_at DESC
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(query.id)
        .bind(query.user_id)
        .bind(query.status)
        .bind(query.channel.as_deref())
        .bind(query.channel_no.as_deref())
        .bind(query.created_after)
        .bind(query.created_before)
        .bind(has_skus)
        .bind(query.extra_info)
        .bind(query.item_extra_info)
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
                unit_price, list_price, discount_amount, payable_amount, is_refunded, extra_info
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
                unit_price, list_price, discount_amount, payable_amount, is_refunded, extra_info
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
                discount_amount, payable_amount, channel_fee,
                extra_info
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9
            )
            "#,
        )
        .bind(args.id)
        .bind(args.user_id)
        .bind(args.channel)
        .bind(args.channel_no)
        .bind(OrderStatus::Pending)
        .bind(args.discount_amount)
        .bind(args.payable_amount)
        .bind(args.channel_fee)
        .bind(
            args.extra_info
                .unwrap_or_else(|| serde_json::Value::Object(Default::default())),
        )
        .execute(&mut *conn)
        .await?;

        let item_ids: Vec<Uuid> = args.items.iter().map(|i| i.id).collect();
        let sku_type: Vec<String> = args.items.iter().map(|i| i.sku_type.clone()).collect();
        let sku_id: Vec<Uuid> = args.items.iter().map(|i| i.sku_id).collect();
        let list_price: Vec<i64> = args.items.iter().map(|i| i.list_price).collect();
        let unit_price: Vec<i64> = args.items.iter().map(|i| i.unit_price).collect();
        let discount_amount: Vec<i64> = args.items.iter().map(|i| i.discount_amount).collect();
        let payable_amount: Vec<i64> = args.items.iter().map(|i| i.payable_amount).collect();
        let extra_info: Vec<Option<Value>> =
            args.items.iter().map(|i| i.extra_info.clone()).collect();

        sqlx::query(
            r#"
            WITH new_items AS (
                SELECT *
                FROM UNNEST($2, $3, $4, $5, $6, $7, $8, $9)
                    AS t (id, sku_id, sku_type, list_price, unit_price, discount_amount, payable_amount, extra_info)
            )
            INSERT INTO jidan.order_items (
                id, order_id, sku_id, sku_type,
                list_price, unit_price, discount_amount, payable_amount, extra_info
            )
            SELECT
                id,
                $1 AS order_id,
                sku_id, sku_type, list_price, unit_price, discount_amount, payable_amount, extra_info
            FROM new_items
            "#,
        )
        .bind(args.id)
        .bind(item_ids)
        .bind(sku_id)
        .bind(sku_type)
        .bind(list_price)
        .bind(unit_price)
        .bind(discount_amount)
        .bind(payable_amount)
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
        let result = sqlx::query(
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

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn update_payment(
        &self,
        conn: &mut Self::Context,
        args: OrderPaymentUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
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

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn update_refund(
        &self,
        conn: &mut Self::Context,
        args: OrderRefundUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
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

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn update_extra_info(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
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

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn update_item_extra_info(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
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

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn mark_item_refunded(
        &self,
        conn: &mut Self::Context,
        order_id: Uuid,
        item_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE jidan.order_items
            SET is_refunded = true
            WHERE id = $1 AND order_id = $2 AND is_refunded = false
            "#,
        )
        .bind(item_id)
        .bind(order_id)
        .execute(&mut *conn)
        .await?;

        if result.rows_affected() == 0 {
            return Err(sqlx::Error::RowNotFound);
        }

        Ok(())
    }

    async fn close_expired_orders(&self, conn: &mut Self::Context) -> Result<u64, sqlx::Error> {
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
        .bind(OrderStatus::Closed)
        .bind(OrderStatus::Pending)
        .execute(&mut *conn)
        .await?;

        Ok(result.rows_affected())
    }
}
