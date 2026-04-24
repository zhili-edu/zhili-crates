use serde::{Deserialize, Serialize};
use uuid::Uuid;

mod builder;
mod event;
pub mod migration;
pub mod psp;
mod repo;
mod svc;
mod utils;

pub use repo::PaymentQuery;
pub use svc::PaymentService;

#[repr(i16)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Provider {
    WxpayJsapi = 0,
    WxpayNative = 1,
    Cash = 100,
}

impl sqlx::Type<sqlx::Postgres> for Provider {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i16 as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <i16 as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for Provider {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let val = *self as i16;
        <i16 as sqlx::Encode<sqlx::Postgres>>::encode(val, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for Provider {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let val: i16 = <i16 as sqlx::Decode<sqlx::Postgres>>::decode(value)?;

        match val {
            0 => Ok(Provider::WxpayJsapi),
            1 => Ok(Provider::WxpayNative),
            100 => Ok(Provider::Cash),
            _ => Err(format!("Invalid Provider value: {}", val).into()),
        }
    }
}

#[repr(i16)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Pending = 0,
    Success = 10,
    Failed = 20,
    Refunded = 30,
    Closed = 40,
}

impl sqlx::Type<sqlx::Postgres> for PaymentStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i16 as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <i16 as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for PaymentStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let val = *self as i16;
        <i16 as sqlx::Encode<sqlx::Postgres>>::encode(val, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for PaymentStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let val: i16 = <i16 as sqlx::Decode<sqlx::Postgres>>::decode(value)?;

        match val {
            0 => Ok(PaymentStatus::Pending),
            10 => Ok(PaymentStatus::Success),
            20 => Ok(PaymentStatus::Failed),
            30 => Ok(PaymentStatus::Refunded),
            40 => Ok(PaymentStatus::Closed),
            _ => Err(format!("Invalid PaymentStatus value: {}", val).into()),
        }
    }
}

#[repr(i16)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefundStatus {
    Pending = 0,
    Success = 10,
    Failed = 20,
}

impl sqlx::Type<sqlx::Postgres> for RefundStatus {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i16 as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <i16 as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for RefundStatus {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let val = *self as i16;
        <i16 as sqlx::Encode<sqlx::Postgres>>::encode(val, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for RefundStatus {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let val: i16 = <i16 as sqlx::Decode<sqlx::Postgres>>::decode(value)?;

        match val {
            0 => Ok(RefundStatus::Pending),
            10 => Ok(RefundStatus::Success),
            20 => Ok(RefundStatus::Failed),
            _ => Err(format!("Invalid RefundStatus value: {}", val).into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecordSuccessfulPaymentRequest {
    pub description: String,
    pub amount: i64,
    pub biz_id: Uuid,
    pub provider: Provider,
    pub provider_trade_no: String,
    pub success_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct RecordCashPaymentRequest {
    pub description: String,
    pub amount: i64,
    pub biz_id: Uuid,
    pub receipt_no: Option<String>,
    pub collected_at: time::OffsetDateTime,
    pub operator_id: Option<Uuid>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecordCashRefundRequest {
    pub payment_id: Uuid,
    pub amount: i64,
    pub reason: Option<String>,
    pub refund_no: Option<String>,
    pub refunded_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct PayCallbackResult {
    pub payment_id: Uuid,
    pub biz_id: Uuid,
    pub amount: i64,
    pub status: PaymentStatus,
    pub provider_trade_no: String,
    pub success_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone)]
pub struct RefundCallbackResult {
    pub refund_id: Uuid,
    pub payment_id: Uuid,
    pub biz_id: Uuid,
    pub amount: i64,
    pub status: RefundStatus,
    pub provider_refund_no: String,
    pub success_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PaymentRecord {
    pub id: Uuid,
    pub provider_trade_no: Option<String>,
    pub amount: i64,
    pub refunded_amount: i64,
    pub biz_id: Uuid,
    pub provider: Provider,
    pub provider_info: serde_json::Value,
    pub status: PaymentStatus,
    pub success_at: Option<time::OffsetDateTime>,
    pub expire_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RefundRecord {
    pub id: Uuid,
    pub provider_refund_no: Option<String>,
    pub payment_id: Uuid,
    pub amount: i64,
}
