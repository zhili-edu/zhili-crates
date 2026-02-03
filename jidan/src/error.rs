use thiserror::Error;

use crate::OrderStatus;

#[derive(Debug, Error)]
pub enum CreateOrderError {
    #[error("订单项不能为空")]
    EmptyItems,

    #[error("金额不能为负数: {field} = {value}")]
    NegativeAmount { field: String, value: i64 },

    #[error("折扣金额异常: 折扣 {discount}, 商品总额 {total}")]
    InvalidDiscount { discount: i64, total: i64 },

    #[error("应付款金额异常: 应付 {payable}")]
    InvalidPayableAmount { payable: i64 },

    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
pub enum OrderStatusError {
    #[error("订单不存在: {order_id}")]
    NotFound { order_id: uuid::Uuid },

    #[error("订单状态不允许: 订单 {order_id}, 当前 {current:?}, 期望 {expected:?}")]
    InvalidStatus {
        order_id: uuid::Uuid,
        current: OrderStatus,
        expected: OrderStatus,
    },

    #[error("订单状态不允许: 订单 {order_id}, 当前 {current:?}, 允许 {allowed:?}")]
    InvalidStatusMultiple {
        order_id: uuid::Uuid,
        current: OrderStatus,
        allowed: Vec<OrderStatus>,
    },

    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error("订单不存在: {order_id}")]
    NotFound { order_id: uuid::Uuid },

    #[error("订单状态不允许支付: 订单 {order_id}, 当前 {current:?}, 允许 {allowed:?}")]
    InvalidStatus {
        order_id: uuid::Uuid,
        current: OrderStatus,
        allowed: Vec<OrderStatus>,
    },

    #[error("支付金额超限: 订单 {order_id}, 应付 {payable}, 已付 {paid}, 本次 {amount}")]
    AmountExceedsPayable {
        order_id: uuid::Uuid,
        payable: i64,
        paid: i64,
        amount: i64,
    },

    #[error("支付金额必须为正数: {amount}")]
    InvalidAmount { amount: i64 },

    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Error)]
pub enum RefundError {
    #[error("订单不存在: {order_id}")]
    NotFound { order_id: uuid::Uuid },

    #[error("退款订单项不能为空")]
    EmptyItems,

    #[error("订单项不存在: {item_id}")]
    ItemNotFound { item_id: uuid::Uuid },

    #[error("订单项已退款: {item_id}")]
    ItemAlreadyRefunded { item_id: uuid::Uuid },

    #[error("退款金额超限: 订单 {order_id}, 已付 {paid}, 已退 {refunded}, 本次 {amount}")]
    AmountExceedsPaid {
        order_id: uuid::Uuid,
        paid: i64,
        refunded: i64,
        amount: i64,
    },

    #[error("退款金额必须为正数: {amount}")]
    InvalidAmount { amount: i64 },

    #[error("数据库错误: {0}")]
    Database(#[from] sqlx::Error),
}
