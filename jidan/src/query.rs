use std::collections::HashMap;

use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{OrderDetail, OrderItemDetail, OrderService, OrderStatus, OrderSummary};

#[derive(Debug, Clone)]
pub struct OrderQuery<'a> {
    pub user_id: Option<Uuid>,
    pub status: Option<OrderStatus>,
    pub channel: Option<String>,
    pub created_after: Option<OffsetDateTime>,
    pub created_before: Option<OffsetDateTime>,
    pub has_items: Option<&'a [Uuid]>,
    pub extra_info: Option<&'a Value>,
    pub offset: i64,
    pub limit: Option<i64>,
}

impl Default for OrderQuery<'_> {
    fn default() -> Self {
        Self {
            user_id: None,
            status: None,
            channel: None,
            created_after: None,
            created_before: None,
            has_items: None,
            extra_info: None,
            offset: 0,
            limit: Some(20),
        }
    }
}

impl<'a> OrderQuery<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn status(mut self, status: OrderStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    pub fn created_after(mut self, t: OffsetDateTime) -> Self {
        self.created_after = Some(t);
        self
    }

    pub fn created_before(mut self, t: OffsetDateTime) -> Self {
        self.created_before = Some(t);
        self
    }

    pub fn has_items(mut self, items: &'a [Uuid]) -> Self {
        self.has_items = Some(items);
        self
    }

    pub fn extra_info(mut self, extra_info: &'a Value) -> Self {
        self.extra_info = Some(extra_info);
        self
    }

    pub fn page(mut self, page: i64, page_size: i64) -> Self {
        self.offset = (page - 1).max(0) * page_size;
        self.limit = Some(page_size);
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = offset;
        self
    }

    pub fn limit(mut self, limit: Option<i64>) -> Self {
        self.limit = limit;
        self
    }
}

fn apply_filters<'a>(builder: &mut QueryBuilder<'a, Postgres>, query: &'a OrderQuery) {
    if let Some(uid) = query.user_id {
        builder.push(" AND user_id = ");
        builder.push_bind(uid);
    }
    if let Some(status) = query.status {
        builder.push(" AND status = ");
        builder.push_bind(status);
    }
    if let Some(channel) = &query.channel {
        builder.push(" AND channel = ");
        builder.push_bind(channel);
    }
    if let Some(after) = query.created_after {
        builder.push(" AND created_at >= ");
        builder.push_bind(after);
    }
    if let Some(before) = query.created_before {
        builder.push(" AND created_at < ");
        builder.push_bind(before);
    }
    if let Some(items) = &query.has_items
        && !items.is_empty()
    {
        builder.push(" AND id IN (SELECT order_id FROM jidan.order_items WHERE item_id = ANY(");
        builder.push_bind(items);
        builder.push("))");
    }
    if let Some(info) = query.extra_info {
        builder.push(" AND extra_info @> ");
        builder.push_bind(info);
    }
}

impl OrderService {
    pub async fn query_orders(
        &self,
        query: OrderQuery<'_>,
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        let mut builder = QueryBuilder::new(
            r#"
            SELECT
                id, user_id, status, channel, channel_no,
                total_items_amount, payable_amount, paid_amount, refunded_amount,
                created_at, expire_at
            FROM jidan.orders
            WHERE 1=1
            "#,
        );

        apply_filters(&mut builder, &query);

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
            .build_query_as::<OrderSummary>()
            .fetch_all(&self.pool)
            .await
    }

    pub async fn query_orders_with_details(
        &self,
        query: OrderQuery<'_>,
    ) -> Result<Vec<OrderDetail>, sqlx::Error> {
        let mut builder = QueryBuilder::new(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount,
                payable_amount, paid_amount, refunded_amount,
                refund_fee,
                created_at, updated_at, expire_at,
                extra_info
            FROM jidan.orders
            WHERE 1=1
            "#,
        );

        apply_filters(&mut builder, &query);

        builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(limit);
        }
        if query.offset > 0 {
            builder.push(" OFFSET ");
            builder.push_bind(query.offset);
        }

        let orders_rows = builder.build().fetch_all(&self.pool).await?;

        if orders_rows.is_empty() {
            return Ok(vec![]);
        }

        let order_ids: Vec<Uuid> = orders_rows.iter().map(|row| row.get("id")).collect();

        // Batch fetch items
        let items_rows = sqlx::query(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info, order_id
            FROM jidan.order_items
            WHERE order_id = ANY($1)
            "#,
        )
        .bind(&order_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut items_map: HashMap<Uuid, Vec<OrderItemDetail>> = HashMap::new();

        for row in items_rows {
            let order_id: Uuid = row.get("order_id");
            let item = OrderItemDetail {
                // id: row.get("id"),
                item_id: row.get("item_id"),
                item_type: row.get("item_type"),
                original_price: row.get("original_price"),
                unit_price: row.get("unit_price"),
                real_amount: row.get("real_amount"),
                extra_info: row.try_get("extra_info").unwrap_or(None),
            };
            items_map.entry(order_id).or_default().push(item);
        }

        let mut results = Vec::with_capacity(orders_rows.len());
        for row in orders_rows {
            let id: Uuid = row.get("id");
            let items = items_map.remove(&id).unwrap_or_default();

            results.push(OrderDetail {
                id,
                user_id: row.get("user_id"),
                channel: row.get("channel"),
                channel_no: row.get("channel_no"),
                status: row.get("status"),
                total_items_amount: row.get("total_items_amount"),
                payment_fee: row.get("payment_fee"),
                discount_amount: row.get("discount_amount"),
                payable_amount: row.get("payable_amount"),
                paid_amount: row.get("paid_amount"),
                refunded_amount: row.get("refunded_amount"),
                refund_fee: row.get("refund_fee"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
                expire_at: row.get("expire_at"),
                extra_info: row.try_get("extra_info").unwrap_or(None),
                items,
            });
        }

        Ok(results)
    }

    pub async fn get_orders_by_user_id(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        self.query_orders(OrderQuery::new().user_id(user_id).limit(None))
            .await
    }

    pub async fn get_orders_by_user_id_and_status(
        &self,
        user_id: Uuid,
        status: OrderStatus,
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        self.query_orders(
            OrderQuery::new()
                .user_id(user_id)
                .status(status)
                .limit(None),
        )
        .await
    }

    pub async fn get_orders_by_user_id_and_channel(
        &self,
        user_id: Uuid,
        channel: &str,
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        self.query_orders(
            OrderQuery::new()
                .user_id(user_id)
                .channel(channel)
                .limit(None),
        )
        .await
    }

    pub async fn get_order_detail_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<OrderDetail>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount,
                payable_amount, paid_amount, refunded_amount,
                refund_fee,
                created_at, updated_at, expire_at,
                extra_info
            FROM jidan.orders
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let items = sqlx::query_as::<_, OrderItemDetail>(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE order_id = $1
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(OrderDetail {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            channel: row.try_get("channel")?,
            channel_no: row.get("channel_no"),
            status: row.try_get("status")?,
            total_items_amount: row.try_get("total_items_amount")?,
            payment_fee: row.try_get("payment_fee")?,
            discount_amount: row.try_get("discount_amount")?,
            payable_amount: row.try_get("payable_amount")?,
            paid_amount: row.try_get("paid_amount")?,
            refunded_amount: row.try_get("refunded_amount")?,
            refund_fee: row.try_get("refund_fee")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
            expire_at: row.try_get("expire_at")?,
            extra_info: row.try_get("extra_info")?,
            items,
        }))
    }

    pub async fn get_order_detail_by_channel_no(
        &self,
        channel: &str,
        channel_no: &str,
    ) -> Result<Option<OrderDetail>, sqlx::Error> {
        let row = sqlx::query(
            r#"
            SELECT
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount,
                payable_amount, paid_amount, refunded_amount,
                refund_fee,
                created_at, updated_at, expire_at,
                extra_info
            FROM jidan.orders
            WHERE channel = $1 AND channel_no = $2
            "#,
        )
        .bind(channel)
        .bind(channel_no)
        .fetch_optional(&self.pool)
        .await?;

        let mut order = match row {
            Some(row) => OrderDetail {
                id: row.try_get("id")?,
                user_id: row.try_get("user_id")?,
                channel: row.try_get("channel")?,
                channel_no: row.get("channel_no"),
                status: row.try_get("status")?,
                total_items_amount: row.try_get("total_items_amount")?,
                payment_fee: row.try_get("payment_fee")?,
                discount_amount: row.try_get("discount_amount")?,
                payable_amount: row.try_get("payable_amount")?,
                paid_amount: row.try_get("paid_amount")?,
                refunded_amount: row.try_get("refunded_amount")?,
                refund_fee: row.try_get("refund_fee")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                expire_at: row.try_get("expire_at")?,
                extra_info: row.try_get("extra_info")?,
                items: vec![],
            },
            None => return Ok(None),
        };

        order.items = sqlx::query_as::<_, OrderItemDetail>(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE order_id = $1
            "#,
        )
        .bind(order.id)
        .fetch_all(&self.pool)
        .await?;

        Ok(Some(order))
    }

    /// 获取创建于 [begin, end) 内的订单
    pub async fn get_orders_created_in(
        &self,
        begin: time::OffsetDateTime,
        end: time::OffsetDateTime,
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        self.query_orders(
            OrderQuery::new()
                .created_after(begin)
                .created_before(end)
                .limit(None),
        )
        .await
    }

    /// 获取所有含有给定物品的订单
    pub async fn get_orders_of_items(
        &self,
        item_ids: &[Uuid],
    ) -> Result<Vec<OrderSummary>, sqlx::Error> {
        self.query_orders(OrderQuery::new().has_items(item_ids).limit(None))
            .await
    }

    /// 获取所有含有给定物品的订单
    pub async fn get_order_id_map_of_items(
        &self,
        item_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Uuid>, sqlx::Error> {
        Ok(sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"
            SELECT id, order_id FROM jidan.order_items WHERE item_id = ANY($1)
            "#,
        )
        .bind(item_ids)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect())
    }

    /// 获取订单内的物品
    pub async fn get_items_of_order(
        &self,
        order_id: Uuid,
    ) -> Result<Vec<OrderItemDetail>, sqlx::Error> {
        sqlx::query_as::<_, OrderItemDetail>(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE order_id = $1
            "#,
        )
        .bind(order_id)
        .fetch_all(&self.pool)
        .await
    }

    /// 获取订单内的物品
    pub async fn get_items_of_orders(
        &self,
        order_ids: &[Uuid],
    ) -> Result<Vec<OrderItemDetail>, sqlx::Error> {
        sqlx::query_as::<_, OrderItemDetail>(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE order_id = ANY($1)
            "#,
        )
        .bind(order_ids)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_items_by_ids(
        &self,
        item_ids: &[Uuid],
    ) -> Result<Vec<OrderItemDetail>, sqlx::Error> {
        sqlx::query_as::<_, OrderItemDetail>(
            r#"
            SELECT
                id, item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM jidan.order_items
            WHERE id = ANY($1)
            "#,
        )
        .bind(item_ids)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn find_all_by_extra_info(
        &self,
        extra_info: &Value,
    ) -> Result<Vec<OrderDetail>, sqlx::Error> {
        self.query_orders_with_details(OrderQuery::new().extra_info(extra_info).limit(None))
            .await
    }

    pub async fn find_optional_by_extra_info(
        &self,
        extra_info: &Value,
    ) -> Result<Option<OrderDetail>, sqlx::Error> {
        let mut orders = self
            .query_orders_with_details(OrderQuery::new().extra_info(extra_info).limit(Some(1)))
            .await?;
        Ok(orders.pop())
    }
}
