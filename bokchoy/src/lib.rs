use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    event::PaymentEventKind,
    psp::{PayRequest, PayResponse, PaymentServiceProvider, RefundRequest, RefundResponse},
};

mod builder;
mod event;
pub mod migration;
pub mod psp;
mod utils;

#[repr(i16)]
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Provider {
    WxpayJsapi = 0,
    WxpayNative = 1,
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
    pub status: PaymentStatus,
}

#[derive(Clone)]
pub struct PaymentService {
    providers: Arc<HashMap<Provider, Box<dyn PaymentServiceProvider + Send + Sync>>>,
}

impl std::fmt::Debug for PaymentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PaymentService")
    }
}

impl PaymentService {
    pub fn builder() -> builder::PaymentServiceBuilder {
        builder::PaymentServiceBuilder::default()
    }

    pub async fn get_successful_payments(
        &self,
        biz_id: Uuid,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<Vec<PaymentRecord>, sqlx::Error> {
        sqlx::query_as::<_, PaymentRecord>(
            r#"
            SELECT
                id, provider_trade_no, amount, refunded_amount,
                biz_id, provider, status
            FROM bokchoy.payments
            WHERE biz_id = $1 AND status = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(biz_id)
        .bind(PaymentStatus::Success)
        .fetch_all(&mut **tx)
        .await
    }

    pub async fn record_successful_payment(
        &self,
        req: RecordSuccessfulPaymentRequest,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO bokchoy.payments (
                id, description, status, amount, refunded_amount,
                biz_id, provider, provider_trade_no, success_at,
                created_at, updated_at
            )
            VALUES (
                uuidv7(), $1, $2, $3, 0,
                $4, $5, $6, $7,
                now(), now()
            )
            "#,
        )
        .bind(req.description)
        .bind(PaymentStatus::Success)
        .bind(req.amount)
        .bind(req.biz_id)
        .bind(req.provider)
        .bind(req.provider_trade_no)
        .bind(req.success_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn pay(
        &self,
        key: Provider,
        req: PayRequest,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> PayResponse {
        let provider = self.providers.get(&key).unwrap();

        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO bokchoy.payments (description, status, amount, biz_id, provider)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(&req.description)
        .bind(PaymentStatus::Pending)
        .bind(req.amount)
        .bind(req.biz_id)
        .bind(key)
        .fetch_one(&mut **tx)
        .await
        .unwrap();

        let (res, http_req, http_res) = provider.pay(id, req).await;

        sqlx::query(
            r#"
            INSERT INTO bokchoy.payment_events (payment_id, kind, http_req, http_res)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(id)
        .bind(PaymentEventKind::PaymentCreate)
        .bind(serde_json::to_value(&http_req).unwrap())
        .bind(http_res.map(|j| serde_json::to_value(&j).unwrap()))
        .execute(&mut **tx)
        .await
        .unwrap();

        res
    }

    pub async fn handle_pay_callback(
        &self,
        key: Provider,
        req: http::Request<bytes::Bytes>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> (PayCallbackResult, http::Response<String>) {
        let provider = self.providers.get(&key).unwrap();

        let (outcome, http_req, http_res) = provider.pay_callback(req).await;

        let (biz_id, amount) = sqlx::query_as::<_, (Uuid, i64)>(
            r#"
            UPDATE bokchoy.payments
            SET
                status = $2, provider_trade_no = $3,
                success_at = $4, updated_at = $4
            WHERE id = $1
            RETURNING biz_id, amount
            "#,
        )
        .bind(outcome.id)
        .bind(PaymentStatus::Success)
        .bind(outcome.provider_trade_no.clone())
        .bind(outcome.success_at)
        .fetch_one(&mut **tx)
        .await
        .unwrap();

        sqlx::query(
            r#"
            INSERT INTO bokchoy.payment_events (payment_id, kind, http_req, http_res)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(outcome.id)
        .bind(PaymentEventKind::PaymentCallback)
        .bind(serde_json::to_value(&http_req).unwrap())
        .bind(http_res.map(|j| serde_json::to_value(&j).unwrap()))
        .execute(&mut **tx)
        .await
        .unwrap();

        (
            PayCallbackResult {
                payment_id: outcome.id,
                biz_id,
                amount,
                status: PaymentStatus::Success,
                provider_trade_no: outcome.provider_trade_no,
                success_at: Some(outcome.success_at),
            },
            outcome.res,
        )
    }

    pub async fn handle_refund_callback(
        &self,
        key: Provider,
        req: http::Request<bytes::Bytes>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> (RefundCallbackResult, http::Response<String>) {
        let provider = self.providers.get(&key).unwrap();

        let (outcome, http_req, http_res) = provider.refund_callback(req).await;

        let (payment_id, amount, provider_refund_no, biz_id) =
            sqlx::query_as::<_, (Uuid, i64, Option<String>, Uuid)>(
                r#"
                UPDATE bokchoy.refunds r
                SET
                    status = $2,
                    success_at = $3,
                    updated_at = now()
                FROM bokchoy.payments p
                WHERE r.id = $1 AND r.payment_id = p.id
                RETURNING r.payment_id, r.amount, r.provider_refund_no, p.biz_id
                "#,
            )
            .bind(outcome.refund_id)
            .bind(outcome.status)
            .bind(outcome.success_at)
            .fetch_one(&mut **tx)
            .await
            .unwrap();
        if outcome.status == RefundStatus::Success {
            sqlx::query(
                r#"
                UPDATE bokchoy.payments
                SET
                    refunded_amount = refunded_amount + $2,
                    updated_at = $3
                WHERE id = $1
                "#,
            )
            .bind(payment_id)
            .bind(amount)
            .bind(outcome.success_at)
            .execute(&mut **tx)
            .await
            .unwrap();
        }

        sqlx::query(
            r#"
            INSERT INTO bokchoy.payment_events (payment_id, kind, http_req, http_res)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(payment_id)
        .bind(PaymentEventKind::RefundCallback)
        .bind(serde_json::to_value(&http_req).unwrap())
        .bind(http_res.map(|j| serde_json::to_value(&j).unwrap()))
        .execute(&mut **tx)
        .await
        .unwrap();

        (
            RefundCallbackResult {
                refund_id: outcome.refund_id,
                payment_id,
                biz_id,
                amount,
                status: outcome.status,
                provider_refund_no: provider_refund_no.unwrap_or_default(),
                success_at: outcome.success_at,
            },
            outcome.res,
        )
    }

    pub async fn refund(
        &self,
        payment_id: Uuid,
        amount: i64,
        reason: Option<String>,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ) -> RefundResponse {
        let (provider_key, total, provider_trade_no) =
            sqlx::query_as::<_, (Provider, i64, Option<String>)>(
                r#"
                SELECT provider, amount, provider_trade_no
                FROM bokchoy.payments
                WHERE id = $1
                "#,
            )
            .bind(payment_id)
            .fetch_one(&mut **tx)
            .await
            .unwrap();

        let provider = self.providers.get(&provider_key).unwrap();

        let refund_id = Uuid::now_v7();

        let req = RefundRequest {
            refund_id,
            provider_trade_no: provider_trade_no.expect("Payment missing provider_trade_no"),
            amount,
            total,
        };

        sqlx::query(
            r#"
            INSERT INTO bokchoy.refunds (
                id, payment_id,
                amount, reason, status, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, now(), now())
            "#,
        )
        .bind(refund_id)
        .bind(payment_id)
        .bind(amount)
        .bind(reason)
        .bind(RefundStatus::Pending)
        .execute(&mut **tx)
        .await
        .unwrap();

        let (res, http_req, http_res) = provider.refund(payment_id, req).await;

        let status = if res.status == "SUCCESS" {
            RefundStatus::Success
        } else {
            RefundStatus::Pending
        };

        if status == RefundStatus::Success {
            sqlx::query(
                r#"
                UPDATE bokchoy.payments
                SET
                    refunded_amount = refunded_amount + $2,
                    updated_at = now()
                WHERE id = $1
                "#,
            )
            .bind(payment_id)
            .bind(amount)
            .execute(&mut **tx)
            .await
            .unwrap();

            sqlx::query(
                r#"
                UPDATE bokchoy.refunds
                SET
                    provider_refund_no = $2,
                    status = $3,
                    success_at = now()
                WHERE id = $1
                "#,
            )
            .bind(refund_id)
            .bind(&res.provider_refund_no)
            .bind(status)
            .execute(&mut **tx)
            .await
            .unwrap();
        } else {
            sqlx::query(
                r#"
                UPDATE bokchoy.refunds
                SET provider_refund_no = $2, status = $3
                WHERE id = $1
                "#,
            )
            .bind(refund_id)
            .bind(&res.provider_refund_no)
            .bind(status)
            .execute(&mut **tx)
            .await
            .unwrap();
        }

        sqlx::query(
            r#"
            INSERT INTO bokchoy.payment_events (payment_id, kind, http_req, http_res)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(payment_id)
        .bind(PaymentEventKind::PaymentRefund)
        .bind(serde_json::to_value(&http_req).unwrap())
        .bind(http_res.map(|j| serde_json::to_value(&j).unwrap()))
        .execute(&mut **tx)
        .await
        .unwrap();

        res
    }
}
