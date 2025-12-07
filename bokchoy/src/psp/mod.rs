use std::collections::HashMap;

use time::OffsetDateTime;
use uuid::Uuid;

use crate::event::{HttpRequestJson, HttpResponseJson};

mod wxpay_jsapi;

pub use wxpay_jsapi::WxPayJsapi;

pub struct PayRequest {
    pub biz_id: Uuid,
    pub amount: i64,
    pub description: String,
    pub extras: HashMap<String, String>,
}

pub struct PayResponse {
    pub provider_params: serde_json::Value,
}

pub struct PayCallbackOutcome {
    pub id: Uuid,
    pub provider_trade_no: String,
    pub success_at: OffsetDateTime,
    pub res: http::Response<String>,
}

pub struct RefundRequest {
    pub refund_id: Uuid,
    pub provider_trade_no: String,
    pub amount: i64,
    pub total: i64,
}

pub struct RefundResponse {
    pub refund_id: Uuid,
    pub provider_refund_no: String,
    pub status: String,
}

pub struct RefundCallbackOutcome {
    pub refund_id: Uuid,
    pub provider_refund_no: String,
    pub success_at: Option<OffsetDateTime>,
    pub status: crate::RefundStatus,
    pub res: http::Response<String>,
}

#[async_trait::async_trait]
pub(crate) trait PaymentServiceProvider: Send + Sync {
    async fn pay(
        &self,
        id: Uuid,
        req: PayRequest,
    ) -> (PayResponse, HttpRequestJson, Option<HttpResponseJson>);

    async fn pay_callback(
        &self,
        req: http::Request<bytes::Bytes>,
    ) -> (
        PayCallbackOutcome,
        HttpRequestJson,
        Option<HttpResponseJson>,
    );

    async fn refund(
        &self,
        id: Uuid,
        req: RefundRequest,
    ) -> (RefundResponse, HttpRequestJson, Option<HttpResponseJson>);

    async fn refund_callback(
        &self,
        req: http::Request<bytes::Bytes>,
    ) -> (
        RefundCallbackOutcome,
        HttpRequestJson,
        Option<HttpResponseJson>,
    );
}
