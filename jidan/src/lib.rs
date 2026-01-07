use uuid::Uuid;

pub mod error;
pub mod migration;
mod model;
mod query;
mod repo;
mod svc;

pub use error::{CreateOrderError, OrderStatusError, PaymentError, RefundError};
pub use model::{CreateOrder, CreateOrderItem, Order, OrderItem, OrderStatus};
pub use query::OrderQuery;
pub use repo::{
    MockOrderRepository, OrderCreateArgs, OrderItemCreateArgs, OrderPaymentUpdateArgs,
    OrderRefundUpdateArgs, OrderRepo, OrderRepository,
};
pub use svc::OrderService;

#[derive(Debug, Clone)]
pub struct PaymentResult {
    pub order_id: Uuid,
    pub previous_status: OrderStatus,
    pub current_status: OrderStatus,
    pub paid_amount: i64,
    pub payable_amount: i64,
}

impl PaymentResult {
    pub fn is_fulfilled(&self) -> bool {
        matches!(self.current_status, OrderStatus::Fulfilled)
    }

    pub fn just_fulfilled(&self) -> bool {
        !matches!(self.previous_status, OrderStatus::Fulfilled) && self.is_fulfilled()
    }
}

#[derive(Debug, Clone)]
pub struct RefundResult {
    pub order_id: Uuid,
    pub previous_status: OrderStatus,
    pub current_status: OrderStatus,
    pub refunded_amount: i64,
    pub paid_amount: i64,
}

impl RefundResult {
    pub fn is_fully_refunded(&self) -> bool {
        matches!(self.current_status, OrderStatus::Refunded)
    }
}
