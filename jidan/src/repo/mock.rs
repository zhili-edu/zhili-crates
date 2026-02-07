use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{Order, OrderItem, OrderQuery, OrderStatus};

fn json_contains(target: &Value, filter: &Value) -> bool {
    match (target, filter) {
        (Value::Object(target_map), Value::Object(filter_map)) => {
            filter_map.iter().all(|(k, v)| {
                target_map
                    .get(k)
                    .is_some_and(|target_val| json_contains(target_val, v))
            })
        }
        (Value::Array(target_arr), Value::Array(filter_arr)) => filter_arr
            .iter()
            .all(|needle| target_arr.iter().any(|item| item == needle)),
        _ => target == filter,
    }
}

fn json_merge(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            let mut merged = base_map.clone();
            for (k, v) in patch_map {
                merged.insert(k.clone(), v.clone());
            }
            Value::Object(merged)
        }
        (Value::Array(base_arr), Value::Array(patch_arr)) => {
            let mut merged = base_arr.clone();
            merged.extend(patch_arr.iter().cloned());
            Value::Array(merged)
        }
        _ => patch.clone(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct MockOrderRepository {
    pub orders: Arc<RwLock<HashMap<Uuid, Order>>>,
    order_items: Arc<RwLock<HashMap<Uuid, (OrderItem, Uuid)>>>, // (item, order_id)
}

impl MockOrderRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&self) {
        let orders = self.orders.clone();
        let items = self.order_items.clone();
        tokio::spawn(async move {
            orders.write().await.clear();
            items.write().await.clear();
        });
    }
}

#[async_trait]
impl super::OrderRepository for MockOrderRepository {
    type Context = ();

    async fn query(
        &self,
        _conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        let orders = self.orders.read().await;
        let items = self.order_items.read().await;
        let mut result: Vec<Order> = orders
            .values()
            .filter(|order| {
                if let Some(id) = query.id {
                    if order.id != id {
                        return false;
                    }
                }
                if let Some(user_id) = query.user_id {
                    if order.user_id != user_id {
                        return false;
                    }
                }
                if let Some(status) = query.status {
                    if order.status != status {
                        return false;
                    }
                }
                if let Some(channel) = &query.channel {
                    if &order.channel != channel {
                        return false;
                    }
                }
                if let Some(channel_no) = &query.channel_no {
                    if order.channel_no.as_ref() != Some(channel_no) {
                        return false;
                    }
                }
                if let Some(after) = query.created_after {
                    if order.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = query.created_before {
                    if order.created_at >= before {
                        return false;
                    }
                }
                if let Some(extra) = query.extra_info {
                    if !json_contains(&order.extra_info, extra) {
                        return false;
                    }
                }
                if let Some(sku_ids) = &query.has_skus {
                    if !sku_ids.is_empty()
                        && !items.values().any(|(item, _)| {
                            item.order_id == order.id && sku_ids.contains(&item.sku_id)
                        })
                    {
                        return false;
                    }
                }
                if let Some(item_info) = query.item_extra_info {
                    if !items.values().any(|(item, _)| {
                        item.order_id == order.id && json_contains(&item.extra_info, item_info)
                    }) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let offset = query.offset.max(0) as usize;
        let limit = query.limit.unwrap_or(i64::MAX);

        if offset > 0 {
            if offset >= result.len() {
                return Ok(Vec::new());
            }
            result = result.split_off(offset);
        }

        if limit <= 0 {
            return Ok(Vec::new());
        }
        let limit = limit as usize;
        if result.len() > limit {
            result.truncate(limit);
        }

        Ok(result)
    }

    async fn query_one(
        &self,
        _conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        let orders = self.orders.read().await;
        let items = self.order_items.read().await;
        let mut result: Vec<Order> = orders
            .values()
            .filter(|order| {
                if let Some(id) = query.id {
                    if order.id != id {
                        return false;
                    }
                }
                if let Some(user_id) = query.user_id {
                    if order.user_id != user_id {
                        return false;
                    }
                }
                if let Some(status) = query.status {
                    if order.status != status {
                        return false;
                    }
                }
                if let Some(channel) = &query.channel {
                    if &order.channel != channel {
                        return false;
                    }
                }
                if let Some(channel_no) = &query.channel_no {
                    if order.channel_no.as_ref() != Some(channel_no) {
                        return false;
                    }
                }
                if let Some(after) = query.created_after {
                    if order.created_at < after {
                        return false;
                    }
                }
                if let Some(before) = query.created_before {
                    if order.created_at >= before {
                        return false;
                    }
                }
                if let Some(extra) = query.extra_info {
                    if !json_contains(&order.extra_info, extra) {
                        return false;
                    }
                }
                if let Some(sku_ids) = &query.has_skus {
                    if !sku_ids.is_empty()
                        && !items.values().any(|(item, _)| {
                            item.order_id == order.id && sku_ids.contains(&item.sku_id)
                        })
                    {
                        return false;
                    }
                }
                if let Some(item_info) = query.item_extra_info {
                    if !items.values().any(|(item, _)| {
                        item.order_id == order.id && json_contains(&item.extra_info, item_info)
                    }) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result.into_iter().next())
    }

    async fn query_one_for_update(
        &self,
        conn: &mut Self::Context,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        self.query_one(conn, query).await
    }

    async fn get_orders_items(
        &self,
        _conn: &mut Self::Context,
        order_ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let items = self.order_items.read().await;
        let result: Vec<OrderItem> = items
            .values()
            .filter(|(item, _)| order_ids.contains(&item.order_id))
            .map(|(item, _)| item.clone())
            .collect();
        Ok(result)
    }

    async fn get_items_by_ids(
        &self,
        _conn: &mut Self::Context,
        item_ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        let items = self.order_items.read().await;
        let result: Vec<OrderItem> = items
            .values()
            .filter(|(item, _)| item_ids.contains(&item.id))
            .map(|(item, _)| item.clone())
            .collect();
        Ok(result)
    }

    async fn create(
        &self,
        _conn: &mut Self::Context,
        args: super::OrderCreateArgs,
    ) -> Result<Uuid, sqlx::Error> {
        use time::OffsetDateTime;

        let now = OffsetDateTime::now_utc();
        let order = Order {
            id: args.id,
            user_id: args.user_id,
            channel: args.channel,
            channel_no: args.channel_no,
            status: OrderStatus::Pending,
            discount_amount: args.discount_amount,
            payable_amount: args.payable_amount,
            paid_amount: 0,
            refunded_amount: 0,
            channel_fee: args.channel_fee,
            created_at: now,
            updated_at: now,
            expire_at: None,
            extra_info: args
                .extra_info
                .unwrap_or(serde_json::Value::Object(Default::default())),
        };

        let mut orders = self.orders.write().await;
        orders.insert(args.id, order.clone());

        let mut items = self.order_items.write().await;
        for item in args.items {
            items.insert(
                item.id,
                (
                    OrderItem {
                        id: item.id,
                        sku_id: item.sku_id,
                        sku_type: item.sku_type,
                        order_id: args.id,
                        unit_price: item.unit_price,
                        list_price: item.list_price,
                        discount_amount: item.discount_amount,
                        payable_amount: item.payable_amount,
                        is_refunded: false,
                        extra_info: item
                            .extra_info
                            .unwrap_or(serde_json::Value::Object(Default::default())),
                    },
                    item.order_id,
                ),
            );
        }

        Ok(args.id)
    }

    async fn update_status(
        &self,
        _conn: &mut Self::Context,
        order_id: Uuid,
        status: OrderStatus,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        match orders.get_mut(&order_id) {
            Some(order) => {
                order.status = status;
                order.updated_at = time::OffsetDateTime::now_utc();
                Ok(())
            }
            None => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn update_payment(
        &self,
        _conn: &mut Self::Context,
        args: super::OrderPaymentUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        match orders.get_mut(&args.order_id) {
            Some(order) => {
                order.paid_amount = args.new_paid_amount;
                order.status = args.new_status;
                order.updated_at = time::OffsetDateTime::now_utc();
                Ok(())
            }
            None => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn update_refund(
        &self,
        _conn: &mut Self::Context,
        args: super::OrderRefundUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        match orders.get_mut(&args.order_id) {
            Some(order) => {
                order.refunded_amount = args.new_refunded_amount;
                order.status = args.new_status;
                order.updated_at = time::OffsetDateTime::now_utc();
                Ok(())
            }
            None => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn update_extra_info(
        &self,
        _conn: &mut Self::Context,
        order_id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        match orders.get_mut(&order_id) {
            Some(order) => {
                order.extra_info = json_merge(&order.extra_info, &extra_info);
                order.updated_at = time::OffsetDateTime::now_utc();
                Ok(())
            }
            None => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn update_item_extra_info(
        &self,
        _conn: &mut Self::Context,
        item_id: Uuid,
        extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        let mut items = self.order_items.write().await;
        match items.get_mut(&item_id) {
            Some((item, _)) => {
                item.extra_info = json_merge(&item.extra_info, &extra_info);
                Ok(())
            }
            None => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn mark_item_refunded(
        &self,
        _conn: &mut Self::Context,
        order_id: Uuid,
        item_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        let mut items = self.order_items.write().await;
        match items.get_mut(&item_id) {
            Some((item, _)) if item.order_id == order_id && !item.is_refunded => {
                item.is_refunded = true;
                Ok(())
            }
            _ => Err(sqlx::Error::RowNotFound),
        }
    }

    async fn close_expired_orders(&self, _conn: &mut Self::Context) -> Result<u64, sqlx::Error> {
        let mut orders = self.orders.write().await;
        let mut count = 0;
        let now = time::OffsetDateTime::now_utc();

        for order in orders.values_mut() {
            if order.status == OrderStatus::Pending
                && let Some(expire_at) = order.expire_at
                && expire_at < now
            {
                order.status = OrderStatus::Closed;
                order.updated_at = now;
                count += 1;
            }
        }

        Ok(count)
    }
}
