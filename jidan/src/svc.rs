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
        let total_items_amount: i64 = info.items.iter().map(|i| i.unit_price).sum();
        let payment_fee: i64 = info.payment_fee.unwrap_or(0);
        let discount_amount: i64 = info.discount_amount.unwrap_or(0);

        if payment_fee < 0 {
            return Err(crate::CreateOrderError::NegativeAmount {
                field: "payment_fee".to_string(),
                value: payment_fee,
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

        let payable_amount: i64 = total_items_amount + payment_fee - discount_amount;

        if payable_amount < 0 {
            return Err(crate::CreateOrderError::InvalidPayableAmount {
                payable: payable_amount,
            });
        }

        let items: Vec<repo::OrderItemCreateArgs> = info
            .items
            .into_iter()
            .map(|item| repo::OrderItemCreateArgs {
                id: Uuid::now_v7(),
                order_id,
                sku_id: item.sku_id,
                sku_type: item.sku_type,
                original_price: item.original_price,
                unit_price: item.unit_price,
                real_amount: item.real_amount,
                extra_info: item.extra_info,
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
                    payment_fee,
                    discount_amount,
                    total_items_amount,
                    payable_amount,
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
            .map_err(crate::OrderStatusError::Database)
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
            .query_order_optional(conn, &OrderQuery::new().id(order_id))
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

    /// 记录退款金额，并根据退款情况更新订单状态
    /// 如果已退款金额 >= 已付金额，状态将更新为 Refunded
    pub async fn add_refund(
        &self,
        order_id: Uuid,
        refund_amount: i64,
        conn: &mut C,
    ) -> Result<RefundResult, crate::RefundError> {
        if refund_amount <= 0 {
            return Err(crate::RefundError::InvalidAmount {
                amount: refund_amount,
            });
        }

        let order = self
            .repo
            .query_order_optional(conn, &OrderQuery::new().id(order_id))
            .await
            .map_err(crate::RefundError::Database)?
            .ok_or_else(|| crate::RefundError::NotFound { order_id })?;

        let new_refunded_amount = order.refunded_amount + refund_amount;

        if new_refunded_amount > order.paid_amount {
            return Err(crate::RefundError::AmountExceedsPaid {
                order_id,
                paid: order.paid_amount,
                refunded: order.refunded_amount,
                amount: refund_amount,
            });
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

    /// 扫描并取消所有已过期的订单 (expire_at < now)
    /// 仅针对 Pending 状态的订单生效
    /// 返回修改的订单数
    pub async fn cancel_expired_orders(&self, conn: &mut C) -> Result<u64, sqlx::Error> {
        self.repo.cancel_expired_orders(conn).await
    }

    /// 将 Fulfilled 状态的订单手动标记为 Completed
    pub async fn complete_order(
        &self,
        order_id: Uuid,
        conn: &mut C,
    ) -> Result<(), crate::OrderStatusError> {
        let order = self
            .repo
            .query_order_optional(conn, &OrderQuery::new().id(order_id))
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
            .map_err(crate::OrderStatusError::Database)?;

        if let Some(patch) = extra_info_patch {
            self.repo
                .update_extra_info(conn, order_id, patch)
                .await
                .map_err(crate::OrderStatusError::Database)?;
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
    pub async fn query_orders(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<Vec<Order>, sqlx::Error> {
        self.repo.query_orders(conn, query).await
    }

    pub async fn query_order_optional(
        &self,
        conn: &mut C,
        query: &OrderQuery<'_>,
    ) -> Result<Option<Order>, sqlx::Error> {
        self.repo.query_order_optional(conn, query).await
    }

    /// 获取单个订单及其items
    pub async fn get_order_with_items(
        &self,
        conn: &mut C,
        order_id: Uuid,
    ) -> Result<Option<(Order, Vec<OrderItem>)>, sqlx::Error> {
        let order = self
            .query_order_optional(conn, &OrderQuery::new().id(order_id))
            .await?;

        match order {
            Some(o) => {
                let items = self.get_order_items(conn, o.id).await?;
                Ok(Some((o, items)))
            }
            None => Ok(None),
        }
    }

    /// 根据sku_ids查询订单
    pub async fn query_orders_by_skus(
        &self,
        conn: &mut C,
        sku_ids: &[Uuid],
    ) -> Result<Vec<Order>, sqlx::Error> {
        if sku_ids.is_empty() {
            return Ok(vec![]);
        }
        self.query_orders(conn, &OrderQuery::new().has_skus(sku_ids))
            .await
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
                original_price: 10000,
                unit_price: 10000,
                real_amount: 10000,
                extra_info: None,
            }],
            payment_fee: Some(100),
            discount_amount: Some(0),
            extra_info: None,
        }
    }

    async fn create_and_get_order(service: &OrderService<()>, user_id: Uuid) -> (Uuid, Order) {
        let create_info = create_test_order(user_id);

        let order_id = service.create_order(create_info, &mut ()).await.unwrap();

        let order = service
            .query_order_optional(&mut (), &OrderQuery::new().id(order_id))
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
            .add_payment(order_id, 5100, &mut conn)
            .await
            .unwrap();

        assert_eq!(result.paid_amount, 10100);
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
    async fn test_create_order_with_negative_payment_fee() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let mut info = create_test_order(user_id);
        info.payment_fee = Some(-100);

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

        let result = service.add_refund(order_id, 10000, &mut conn).await;

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
    async fn test_negative_refund() {
        let (service, _) = create_test_service();
        let user_id = Uuid::now_v7();
        let (order_id, _) = create_and_get_order(&service, user_id).await;

        let mut conn = ();
        service
            .add_payment(order_id, 5000, &mut conn)
            .await
            .unwrap();

        let result = service.add_refund(order_id, -100, &mut conn).await;

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(crate::RefundError::InvalidAmount { amount: -100 })
        ));
    }
}
