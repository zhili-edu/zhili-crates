use serde::Serialize;
use serde_json::Value;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, FromRow, Clone)]
pub struct OrderItem {
    pub id: Uuid,
    pub sku_id: Uuid,
    pub sku_type: String,
    pub order_id: Uuid,
    pub unit_price: i64,
    pub list_price: i64,
    pub discount_amount: i64,
    pub payable_amount: i64,
    pub is_refunded: bool,
    pub extra_info: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct Order {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,
    pub status: OrderStatus,

    pub discount_amount: i64,
    pub payable_amount: i64,
    pub paid_amount: i64,
    pub refunded_amount: i64,
    pub channel_fee: i64,

    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub expire_at: Option<OffsetDateTime>,
    pub extra_info: Value,
}

#[repr(i16)]
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// 订单已生成
    Pending = 0,

    /// 订单因某种原因正在处理中，如：
    /// - 等待用户付尾款
    /// - 等待订单审批
    Processing = 10,

    /// 订单被挂起，往往需人工介入
    Suspended = 15,

    /// 订单已经完成所有流程，等待被“使用”
    Fulfilled = 20,

    /// 终结态
    Completed = 30,

    /// 异常终结态：
    /// - 用户主动取消
    Canceled = 40,

    /// 异常终结态：
    /// - 审核不通过
    /// - 过期未付款
    Closed = 45,

    /// 异常终结态：用户选择整单完全退款
    Refunded = 50,
}

impl sqlx::Type<sqlx::Postgres> for OrderStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i16 as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <i16 as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for OrderStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let val = *self as i16;
        <i16 as sqlx::Encode<sqlx::Postgres>>::encode(val, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for OrderStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let val: i16 = <i16 as sqlx::Decode<sqlx::Postgres>>::decode(value)?;

        match val {
            0 => Ok(OrderStatus::Pending),
            10 => Ok(OrderStatus::Processing),
            15 => Ok(OrderStatus::Suspended),
            20 => Ok(OrderStatus::Fulfilled),
            30 => Ok(OrderStatus::Completed),
            40 => Ok(OrderStatus::Canceled),
            45 => Ok(OrderStatus::Closed),
            50 => Ok(OrderStatus::Refunded),
            _ => Err(format!("Invalid OrderStatus value: {}", val).into()),
        }
    }
}

#[derive(Clone)]
pub struct CreateOrder {
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,

    pub items: Vec<CreateOrderItem>,

    pub channel_fee: i64,
    pub discount_amount: Option<i64>,

    pub extra_info: Option<serde_json::Value>,
}

#[derive(Clone)]
pub struct CreateOrderItem {
    pub sku_type: String,
    pub sku_id: Uuid,

    pub list_price: i64,
    pub unit_price: i64,

    pub extra_info: Option<serde_json::Value>,
}
