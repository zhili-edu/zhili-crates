use std::{collections::HashMap, sync::Arc};

use crate::{
    CreateOrder, Order, OrderItem, OrderQuery, OrderRepo, OrderRepository, OrderStatus,
    PaymentResult, RefundResult, repo,
};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug)]
pub struct OrderService<C = sqlx::PgConnection> {
    repo: Arc<dyn OrderRepository<Context = C> + Send + Sync>,
}

impl<C> Clone for OrderService<C> {
    fn clone(&self) -> Self {
        Self {
            repo: Arc::clone(&self.repo),
        }
    }
}

impl Default for OrderService<sqlx::PgConnection> {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderService<sqlx::PgConnection> {
    pub fn new() -> Self {
        Self {
            repo: Arc::new(OrderRepo),
        }
    }
}

impl<C> OrderService<C> {
    pub fn with_repo(repo: Arc<dyn OrderRepository<Context = C> + Send + Sync>) -> Self {
        Self { repo }
    }
}

impl<C> OrderService<C> {
    pub async fn create_order_with_id(
        &self,
        order_id: Uuid,
        info: CreateOrder,
        conn: &mut C,
    ) -> Result<Uuid, crate::CreateOrderError> {
        if info.items.is_empty() {
            return Err(crate::CreateOrderError::EmptyItems);
        }
        let total_items_amount: i64 = info.items.iter().map(|i| i.unit_price).sum();
        let channel_fee: i64 = info.channel_fee;
        let discount_amount: i64 = info.discount_amount.unwrap_or(0);

        if channel_fee < 0 {
            return Err(crate::CreateOrderError::NegativeAmount {
                field: "channel_fee".to_string(),
                value: channel_fee,
            });
        }

        if discount_amount < 0 {
            return Err(crate::CreateOrderError::NegativeAmount {
                field: "discount_amount".to_string(),
                value: discount_amount,
            });
        }

        if discount_amount > total_items_amount {
            return Err(crate::CreateOrderError::InvalidDiscount {
                discount: discount_amount,
                total: total_items_amount,
            });
        }

        let payable_amount: i64 = total_items_amount - discount_amount;

        if payable_amount < 0 {
            return Err(crate::CreateOrderError::InvalidPayableAmount {
                payable: payable_amount,
            });
        }

        let mut per_item_discount: Vec<i64> = vec![0; info.items.len()];
        if total_items_amount > 0 && discount_amount > 0 {
            let mut base_sum: i64 = 0;
            for (idx, item) in info.items.iter().enumerate() {
                let base = ((item.unit_price as i128 * discount_amount as i128)
                    / total_items_amount as i128) as i64;
                per_item_discount[idx] = base;
                base_sum += base;
            }

            let remainder = discount_amount - base_sum;
            if remainder > 0 {
                let mut indices: Vec<usize> = (0..info.items.len()).collect();
                indices.sort_by(|&a, &b| {
                    info.items[b]
                        .unit_price
                        .cmp(&info.items[a].unit_price)
                        .then(a.cmp(&b))
                });
                for idx in indices.into_iter().take(remainder as usize) {
                    per_item_discount[idx] += 1;
                }
            }
        }

        let items: Vec<repo::OrderItemCreateArgs> = info
            .items
            .into_iter()
            .enumerate()
            .map(|(idx, item)| {
                let item_discount = per_item_discount[idx];
                repo::OrderItemCreateArgs {
                    id: Uuid::now_v7(),
                    order_id,
                    sku_id: item.sku_id,
                    sku_type: item.sku_type,
                    list_price: item.list_price,
                    unit_price: item.unit_price,
                    discount_amount: item_discount,
                    payable_amount: item.unit_price - item_discount,
                    extra_info: item.extra_info,
                }
            })
            .collect();

        self.repo
            .create(
                conn,
                repo::OrderCreateArgs {
                    id: order_id,
                    user_id: info.user_id,
                    channel: info.channel,
                    channel_no: info.channel_no,
                    items,
                    discount_amount,
                    payable_amount,
                    channel_fee,
                    expire_at: info.expire_at,
                    extra_info: info.extra_info,
                },
            )
            .await
            .map_err(crate::CreateOrderError::Database)
    }

    pub async fn create_order(
        &self,
        info: CreateOrder,
        conn: &mut C,
    ) -> Result<Uuid, crate::CreateOrderError> {
        let id = Uuid::now_v7();
        self.create_order_with_id(id, info, conn).await?;
        Ok(id)
    }

    /// 不做任何检查，将order设置为 fulfilled 状态
    pub async fn fulfill_order(
        &self,
        order_id: Uuid,
        conn: &mut C,
    ) -> Result<(), crate::OrderStatusError> {
        self.repo
            .update_status(conn, order_id, OrderStatus::Fulfilled)
            .await
            .map_err(|err| match err {
                sqlx::Error::RowNotFound => crate::OrderStatusError::NotFound { order_id },
                _ => crate::OrderStatusError::Database(err),
            })
    }

    /// 记录支付金额，并将订单转换为Processing状态
    /// 如果支付金额达到或超过应付金额，自动转换为Fulfilled状态
    pub async fn add_payment(
        &self,
        order_id: Uuid,
        payment_amount: i64,
        conn: &mut C,
    ) -> Result<PaymentResult, crate::PaymentError> {
        if payment_amount <= 0 {
            return Err(crate::PaymentError::InvalidAmount {
                amount: payment_amount,
            });
        }

        let order = self
            .repo
            .query_one_for_update(conn, &OrderQuery::new(1).id(order_id))
            .await
            .map_err(crate::PaymentError::Database)?
            .ok_or_else(|| crate::PaymentError::NotFound { order_id })?;

        match order.status {
            OrderStatus::Pending | OrderStatus::Processing => {}
            _ => {
                return Err(crate::PaymentError::InvalidStatus {
                    order_id,
                    current: order.status,
                    allowed: vec![OrderStatus::Pending, OrderStatus::Processing],
                });
            }
        }

        let new_paid_amount = order.paid_amount + payment_amount;

        if new_paid_amount > order.payable_amount {
            return Err(crate::PaymentError::AmountExceedsPayable {
                order_id,
                payable: order.payable_amount,
                paid: order.paid_amount,
                amount: payment_amount,
            });
        }

        let new_status = if new_paid_amount >= order.payable_amount {
            OrderStatus::Fulfilled
        } else {
            OrderStatus::Processing
        };

        self.repo
            .update_payment(
                conn,
                repo::OrderPaymentUpdateArgs {
                    order_id,
                    payment_amount,
                    new_paid_amount,
                    new_status,
                },
            )
            .await
            .map_err(crate::PaymentError::Database)?;

        Ok(PaymentResult {
            order_id,
            previous_status: order.status,
            current_status: new_status,
            paid_amount: new_paid_amount,
            payable_amount: order.payable_amount,
        })
    }

    /// 退款多个订单项（每个item仅支持一次退款）
    /// 如果已退款金额 >= 已付金额，状态将更新为 Refunded
    pub async fn refund_items(
        &self,
        order_id: Uuid,
        item_ids: &[Uuid],
        conn: &mut C,
    ) -> Result<RefundResult, crate::RefundError> {
        if item_ids.is_empty() {
            return Err(crate::RefundError::EmptyItems);
        }

        let order = self
            .repo
            .query_one_for_update(conn, &OrderQuery::new(1).id(order_id))
            .await
            .map_err(crate::RefundError::Database)?
            .ok_or_else(|| crate::RefundError::NotFound { order_id })?;

        let mut unique_ids = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for id in item_ids.iter().copied() {
            if seen.insert(id) {
                unique_ids.push(id);
            }
        }

        let items = self
            .repo
            .get_items_by_ids(conn, &unique_ids)
            .await
            .map_err(crate::RefundError::Database)?;

        let mut item_map = std::collections::HashMap::new();
        for item in items {
            item_map.insert(item.id, item);
        }

        let mut refund_amount: i64 = 0;
        for item_id in &unique_ids {
            let item = item_map
                .get(item_id)
                .ok_or_else(|| crate::RefundError::ItemNotFound { item_id: *item_id })?;

            if item.order_id != order_id {
                return Err(crate::RefundError::ItemNotFound { item_id: *item_id });
            }

            if item.is_refunded {
                return Err(crate::RefundError::ItemAlreadyRefunded { item_id: *item_id });
            }

            refund_amount += item.payable_amount;
        }

        if refund_amount <= 0 {
            return Err(crate::RefundError::InvalidAmount {
                amount: refund_amount,
            });
        }

        let new_refunded_amount = order.refunded_amount + refund_amount;

        if new_refunded_amount > order.paid_amount {
            return Err(crate::RefundError::AmountExceedsPaid {
                order_id,
                paid: order.paid_amount,
                refunded: order.refunded_amount,
                amount: refund_amount,
            });
        }

        for item_id in &unique_ids {
            self.repo
                .mark_item_refunded(conn, order_id, *item_id)
                .await
                .map_err(|err| match err {
                    sqlx::Error::RowNotFound => {
                        crate::RefundError::ItemAlreadyRefunded { item_id: *item_id }
                    }
                    _ => crate::RefundError::Database(err),
                })?;
        }

        let new_status = if new_refunded_amount >= order.paid_amount {
            OrderStatus::Refunded
        } else {
            order.status
        };

        self.repo
            .update_refund(
                conn,
                repo::OrderRefundUpdateArgs {
                    order_id,
                    refund_amount,
                    new_refunded_amount,
                    new_status,
                },
            )
            .await
            .map_err(crate::RefundError::Database)?;

        Ok(RefundResult {
            order_id,
            previous_status: order.status,
            current_status: new_status,
            refunded_amount: new_refunded_amount,
            paid_amount: order.paid_amount,
        })
    }

    /// 退款指定订单项（每个item仅支持一次退款）
    pub async fn refund_item(
        &self,
        order_id: Uuid,
        item_id: Uuid,
        conn: &mut C,
    ) -> Result<RefundResult, crate::RefundError> {
        self.refund_items(order_id, std::slice::from_ref(&item_id), conn)
            .await
    }

    /// 扫描并关闭所有已过期的订单 (expire_at < now)
    /// 仅针对 Pending 状态的订单生效
    /// 返回修改的订单数
    pub async fn close_expired_orders(&self, conn: &mut C) -> Result<u64, sqlx::Error> {
        self.repo.close_expired_orders(conn).await
    }

    /// 将 Fulfilled 状态的订单手动标记为 Completed
    pub async fn complete_order(
        &self,
        order_id: Uuid,
        conn: &mut C,
    ) -> Result<(), crate::OrderStatusError> {
        let order = self
            .repo
            .query_one_for_update(conn, &OrderQuery::new(1).id(order_id))
            .await
            .map_err(crate::OrderStatusError::Database)?
            .ok_or_else(|| crate::OrderStatusError::NotFound { order_id })?;

        match order.status {
            OrderStatus::Fulfilled => {}
            _ => {
                return Err(crate::OrderStatusError::InvalidStatus {
                    order_id,
                    current: order.status,
                    expected: OrderStatus::Fulfilled,
                });
            }
        }

        self.repo
            .update_status(conn, order_id, OrderStatus::Completed)
            .await
            .map_err(crate::OrderStatusError::Database)
    }

    pub async fn close_order(
        &self,
        order_id: Uuid,
        extra_info_patch: Option<Value>,
        conn: &mut C,
    ) -> Result<(), crate::OrderStatusError> {
        self.repo
            .update_status(conn, order_id, OrderStatus::Closed)
            .await
            .map_err(|err| match err {
                sqlx::Error::RowNotFound => crate::OrderStatusError::NotFound { order_id },
                _ => crate::OrderStatusError::Database(err),
            })?;

        if let Some(patch) = extra_info_patch {
            self.repo
                .update_extra_info(conn, order_id, patch)
                .await
                .map_err(|err| match err {
                    sqlx::Error::RowNotFound => crate::OrderStatusError::NotFound { order_id },
                    _ => crate::OrderStatusError::Database(err),
                })?;
        }

        Ok(())
    }

    pub async fn update_order_extra_info(
        &self,
        order_id: Uuid,
        extra_info: Value,
        conn: &mut C,
    ) -> Result<(), sqlx::Error> {
        self.repo
            .update_extra_info(conn, order_id, extra_info)
            .await
    }

    pub async fn update_order_item_extra_info(
        &self,
        id: Uuid,
        extra_info: Value,
        conn: &mut C,
    ) -> Result<(), sqlx::Error> {
        self.repo.update_item_extra_info(conn, id, extra_info).await
    }
}

impl<C> OrderService<C> {
    pub async fn count_orders(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<i64, sqlx::Error> {
        self.repo.count(conn, query).await
    }

    pub async fn query_orders(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        self.repo.query(conn, query).await
    }

    pub async fn query_orders_for_update_skip_locked(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        self.repo.query_for_update_skip_locked(conn, query).await
    }

    pub async fn query_order_optional(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        self.repo.query_one(conn, query).await
    }

    /// 获取单个订单及其items
    pub async fn get_order_with_items(
        &self,
        conn: &mut C,
        order_id: Uuid,
    ) -> Result<Option<(Order, Vec<OrderItem>)>, sqlx::Error> {
        let order = self
            .query_order_optional(conn, &OrderQuery::new(1).id(order_id))
            .await?;

        match order {
            Some(o) => {
                let items = self.get_order_items(conn, o.id).await?;
                Ok(Some((o, items)))
            }
            None => Ok(None),
        }
    }

    /// 获取一个订单内的所有订单项
    pub async fn get_order_items(
        &self,
        conn: &mut C,
        order_id: Uuid,
    ) -> Result<Vec<OrderItem>, sqlx::Error> {
        self.repo.get_orders_items(conn, &[order_id]).await
    }

    /// 获取多个订单内的所有订单项，以订单ID为Map
    pub async fn get_orders_items_map(
        &self,
        conn: &mut C,
        order_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<OrderItem>>, sqlx::Error> {
        let items = self.repo.get_orders_items(conn, order_ids).await?;

        let mut map = HashMap::<Uuid, Vec<OrderItem>>::new();
        for item in items {
            map.entry(item.order_id)
                .and_modify(|i| i.push(item.clone()))
                .or_insert_with(|| vec![item]);
        }

        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::MockOrderRepository;
    use crate::{CreateOrder, CreateOrderItem, OrderQuery, OrderStatus};
    use std::sync::Arc;
    use uuid::Uuid;

    fn create_test_service() -> (OrderService<()>, MockOrderRepository) {
        let mock_repo = MockOrderRepository::new();
        let service = OrderService::with_repo(Arc::new(mock_repo.clone()));
        (service, mock_repo)
    }

    fn create_test_order(user_id: Uuid) -> CreateOrder {
        CreateOrder {
            user_id,
            channel: "weapp".to_string(),
            channel_no: Some("NO123456".to_string()),
            items: vec![CreateOrderItem {
                sku_type: "ticket".to_string(),
                sku_id: Uuid::now_v7(),
                list_price: 10000,
                unit_price: 10000,
                extra_info: None,
            }],
            channel_fee: 0,
            discount_amount: Some(0),
            expire_at: None,
            extra_info: None,
        }
    }

    async fn create_and_get_order(service: &OrderService<()>, user_id: Uuid) -> (Uuid, Order) {
        let create_info = create_test_order(user_id);

        let order_id = service.create_order(create_info, &mut ()).await.unwrap();

        let order = service
            .query_order_optional(&mut (), &OrderQuery::new(1).id(order_id))
            .await
            .unwrap()
            .unwrap();
        (order_id, order)
    }

    #[tokio::test]
    async fn test_create_order_with_large_discount() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.discount_amount = Some(20000);

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::CreateOrderError::InvalidDiscount {
                discount: 20000,
                total: 10000
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_discount_greater_than_payable() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.discount_amount = Some(15000);

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::CreateOrderError::InvalidDiscount {
                discount: 15000,
                total: 10000
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_empty_items() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.items = Vec::new();

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(crate::CreateOrderError::EmptyItems)));
    }

    #[tokio::test]
    async fn test_add_zero_payment() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        let result = service.add_payment(order_id, 0, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::PaymentError::InvalidAmount { amount: 0 })
        ));
    }

    #[tokio::test]
    async fn test_add_negative_payment() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        let result = service.add_payment(order_id, -100, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::PaymentError::InvalidAmount { amount: -100 })
        ));
    }

    #[tokio::test]
    async fn test_multiple_partial_payments() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        service
            .add_payment(order_id, 2000, &mut conn)
            .await
            .unwrap();
        service
            .add_payment(order_id, 3000, &mut conn)
            .await
            .unwrap();
        let result = service
            .add_payment(order_id, 5000, &mut conn)
            .await
            .unwrap();

        assert_eq!(result.paid_amount, 10000);
        assert_eq!(result.current_status, OrderStatus::Fulfilled);
    }

    #[tokio::test]
    async fn test_add_payment_panics_on_completed_order() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        service.fulfill_order(order_id, &mut conn).await.unwrap();
        service.complete_order(order_id, &mut conn).await.unwrap();

        let result = service.add_payment(order_id, 1000, &mut conn).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_add_payment_on_canceled_order() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let info = create_test_order(user_id);

        let mut conn = ();
        let order_id = service.create_order(info, &mut conn).await.unwrap();
        service
            .close_order(order_id, None, &mut conn)
            .await
            .unwrap();

        let result = service.add_payment(order_id, 1000, &mut conn).await;
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::PaymentError::InvalidStatus {
                current: crate::OrderStatus::Closed,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_negative_channel_fee() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.channel_fee = -100;

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::CreateOrderError::NegativeAmount {
                field: _,
                value: -100
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_negative_discount() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.discount_amount = Some(-100);

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::CreateOrderError::NegativeAmount {
                field: _,
                value: -100
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_discount_exceeds_total() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.discount_amount = Some(20000);

        let mut conn = ();
        let result = service.create_order(info, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::CreateOrderError::InvalidDiscount {
                discount: 20000,
                total: 10000
            })
        ));
    }

    #[tokio::test]
    async fn test_create_order_with_expire_at() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        let expire_at = time::OffsetDateTime::now_utc() + time::Duration::hours(2);
        info.expire_at = Some(expire_at);

        let mut conn = ();
        let order_id = service.create_order(info, &mut conn).await.unwrap();

        let order = service
            .query_order_optional(&mut conn, &OrderQuery::new(1).id(order_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(order.expire_at, Some(expire_at));
    }

    #[tokio::test]
    async fn test_complete_order_not_fulfilled() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        let result = service.complete_order(order_id, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::OrderStatusError::InvalidStatus {
                current: crate::OrderStatus::Pending,
                expected: crate::OrderStatus::Fulfilled,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn test_refund_exceeds_paid() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        service
            .add_payment(order_id, 5000, &mut conn)
            .await
            .unwrap();

        let items = service.get_order_items(&mut conn, order_id).await.unwrap();
        let item_id = items.first().unwrap().id;

        let result = service.refund_item(order_id, item_id, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::RefundError::AmountExceedsPaid {
                paid: 5000,
                refunded: 0,
                amount: 10000,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn test_refund_item_twice() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        service
            .add_payment(order_id, 10000, &mut conn)
            .await
            .unwrap();

        let items = service.get_order_items(&mut conn, order_id).await.unwrap();
        let item_id = items.first().unwrap().id;

        service
            .refund_item(order_id, item_id, &mut conn)
            .await
            .unwrap();

        let result = service.refund_item(order_id, item_id, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::RefundError::ItemAlreadyRefunded { .. })
        ));
    }

    #[tokio::test]
    async fn test_close_expired_orders_marks_closed() {
        let (service, mock_repo) = create_test_service();
        let user_id = Uuid::now_v7();
        let info = create_test_order(user_id);

        let mut conn = ();
        let order_id = service.create_order(info, &mut conn).await.unwrap();

        {
            let mut orders = mock_repo.orders.write().await;
            let order = orders.get_mut(&order_id).unwrap();
            order.expire_at = Some(time::OffsetDateTime::now_utc() - time::Duration::seconds(1));
        }

        let count = service.close_expired_orders(&mut conn).await.unwrap();
        assert_eq!(count, 1);

        let order = service
            .query_order_optional(&mut conn, &OrderQuery::new(1).id(order_id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(order.status, OrderStatus::Closed);
    }

    #[tokio::test]
    async fn test_query_orders_with_expired_filter() {
        let (service, mock_repo) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        let expired_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let active_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();

        {
            let mut orders = mock_repo.orders.write().await;
            let expired_order = orders.get_mut(&expired_order_id).unwrap();
            expired_order.expire_at =
                Some(time::OffsetDateTime::now_utc() - time::Duration::seconds(1));

            let active_order = orders.get_mut(&active_order_id).unwrap();
            active_order.expire_at =
                Some(time::OffsetDateTime::now_utc() + time::Duration::hours(1));
        }

        let expired = service
            .query_orders(&mut conn, &OrderQuery::new(10).expired(true))
            .await
            .unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].id, expired_order_id);

        let not_expired = service
            .query_orders(&mut conn, &OrderQuery::new(10).expired(false))
            .await
            .unwrap();
        assert_eq!(not_expired.len(), 1);
        assert_eq!(not_expired[0].id, active_order_id);
    }

    #[tokio::test]
    async fn test_query_orders_with_ids_filter() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        let first_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let second_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let ids = [first_order_id, second_order_id];

        let orders = service
            .query_orders(&mut conn, &OrderQuery::new(10).ids(&ids))
            .await
            .unwrap();

        assert_eq!(orders.len(), 2);
        assert!(orders.iter().any(|order| order.id == first_order_id));
        assert!(orders.iter().any(|order| order.id == second_order_id));
    }

    #[tokio::test]
    async fn test_empty_ids_filter_is_ignored_when_other_filters_exist() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();

        let empty_ids: [Uuid; 0] = [];

        let orders = service
            .query_orders(
                &mut conn,
                &OrderQuery::new(10)
                    .ids(&empty_ids)
                    .status(OrderStatus::Pending),
            )
            .await
            .unwrap();

        assert_eq!(orders.len(), 1);
    }

    #[tokio::test]
    async fn test_count_orders_with_status_filter() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        let pending_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let fulfilled_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();

        service
            .add_payment(fulfilled_order_id, 10000, &mut conn)
            .await
            .unwrap();

        let pending_count = service
            .count_orders(&mut conn, &OrderQuery::new(10).status(OrderStatus::Pending))
            .await
            .unwrap();
        let fulfilled_count = service
            .count_orders(
                &mut conn,
                &OrderQuery::new(10).status(OrderStatus::Fulfilled),
            )
            .await
            .unwrap();

        assert_eq!(pending_count, 1);
        assert_eq!(fulfilled_count, 1);
        assert_ne!(pending_order_id, fulfilled_order_id);
    }

    #[tokio::test]
    async fn test_count_orders_with_ids_filter() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        let included_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let _excluded_order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();
        let ids = [included_order_id];

        let count = service
            .count_orders(&mut conn, &OrderQuery::new(10).ids(&ids))
            .await
            .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_count_orders_without_filters_returns_total() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();

        let count = service
            .count_orders(&mut conn, &OrderQuery::new(10))
            .await
            .unwrap();

        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_query_orders_for_update_skip_locked() {
        let (service, mock_repo) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut conn = ();

        let order_id = service
            .create_order(create_test_order(user_id), &mut conn)
            .await
            .unwrap();

        {
            let mut orders = mock_repo.orders.write().await;
            let order = orders.get_mut(&order_id).unwrap();
            order.expire_at = Some(time::OffsetDateTime::now_utc() - time::Duration::seconds(1));
        }

        let orders = service
            .query_orders_for_update_skip_locked(&mut conn, &OrderQuery::new(10).expired(true))
            .await
            .unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].id, order_id);
    }
}
