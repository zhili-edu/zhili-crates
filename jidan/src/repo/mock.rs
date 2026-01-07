use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{Order, OrderItem, OrderQuery, OrderStatus};

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

    async fn query_orders(
        &self,
        _conn: &mut Self::Context,
        _query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        todo!()
    }

    async fn query_order_optional(
        &self,
        _conn: &mut Self::Context,
        _query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        todo!()
    }

    async fn get_orders_items(
        &self,
        _conn: &mut Self::Context,
        _order_ids: &[Uuid],
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        todo!()
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
            total_items_amount: args.total_items_amount,
            payment_fee: args.payment_fee,
            discount_amount: args.discount_amount,
            payable_amount: args.payable_amount,
            paid_amount: 0,
            refunded_amount: 0,
            refund_fee: 0,
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
                        original_price: item.original_price,
                        unit_price: item.unit_price,
                        real_amount: item.real_amount,
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
        if let Some(order) = orders.get_mut(&order_id) {
            order.status = status;
            order.updated_at = time::OffsetDateTime::now_utc();
        }
        Ok(())
    }

    async fn update_payment(
        &self,
        _conn: &mut Self::Context,
        args: super::OrderPaymentUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        if let Some(order) = orders.get_mut(&args.order_id) {
            order.paid_amount = args.new_paid_amount;
            order.status = args.new_status;
            order.updated_at = time::OffsetDateTime::now_utc();
        }
        Ok(())
    }

    async fn update_refund(
        &self,
        _conn: &mut Self::Context,
        args: super::OrderRefundUpdateArgs,
    ) -> Result<(), sqlx::Error> {
        let mut orders = self.orders.write().await;
        if let Some(order) = orders.get_mut(&args.order_id) {
            order.refunded_amount = args.new_refunded_amount;
            order.status = args.new_status;
            order.updated_at = time::OffsetDateTime::now_utc();
        }
        Ok(())
    }

    async fn update_extra_info(
        &self,
        _conn: &mut Self::Context,
        _order_id: Uuid,
        _extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        todo!()
    }

    async fn update_item_extra_info(
        &self,
        _conn: &mut Self::Context,
        _item_id: Uuid,
        _extra_info: Value,
    ) -> Result<(), sqlx::Error> {
        todo!()
    }

    async fn cancel_expired_orders(&self, _conn: &mut Self::Context) -> Result<u64, sqlx::Error> {
        let mut orders = self.orders.write().await;
        let mut count = 0;
        let now = time::OffsetDateTime::now_utc();

        for order in orders.values_mut() {
            if order.status == OrderStatus::Pending {
                if let Some(expire_at) = order.expire_at {
                    if expire_at < now {
                        order.status = OrderStatus::Canceled;
                        order.updated_at = now;
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }
}
