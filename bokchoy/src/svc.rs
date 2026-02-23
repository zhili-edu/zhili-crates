use std::{collections::HashMap, sync::Arc};

use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    PayCallbackResult, PaymentRecord, PaymentStatus, Provider, RecordSuccessfulPaymentRequest,
    RefundCallbackResult, RefundRecord, RefundStatus,
    builder::PaymentServiceBuilder,
    event::PaymentEventKind,
    psp::{PayRequest, PayResponse, PaymentServiceProvider, RefundRequest, RefundResponse},
    repo::{
        PaymentCreate, PaymentEventCreate, PaymentQuery, PaymentRepository, PaymentUpdate,
        RefundCreate, RefundUpdate,
    },
};

pub struct PaymentService<C = PgConnection> {
    pub(crate) providers: Arc<HashMap<Provider, Box<dyn PaymentServiceProvider + Send + Sync>>>,
    pub(crate) repo: Arc<dyn PaymentRepository<Context = C>>,
}

impl<C> Clone for PaymentService<C> {
    fn clone(&self) -> Self {
        Self {
            providers: Arc::clone(&self.providers),
            repo: Arc::clone(&self.repo),
        }
    }
}

impl<C> std::fmt::Debug for PaymentService<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PaymentService")
    }
}

impl<C> PaymentService<C> {
    pub fn builder() -> PaymentServiceBuilder {
        PaymentServiceBuilder::default()
    }

    pub async fn query_payment(
        &self,
        conn: &mut C,
        query: &PaymentQuery,
    ) -> Result<Option<PaymentRecord>, sqlx::Error> {
        self.repo.query_one(conn, query).await
    }

    pub async fn query_payments(
        &self,
        conn: &mut C,
        query: &PaymentQuery,
    ) -> Result<Vec<PaymentRecord>, sqlx::Error> {
        self.repo.query(conn, query).await
    }

    pub async fn query_payment_for_update_skip_locked(
        &self,
        conn: &mut C,
        query: &PaymentQuery,
    ) -> Result<Option<PaymentRecord>, sqlx::Error> {
        self.repo.query_for_update_skip_locked(conn, query).await
    }

    pub async fn get_refunds_by_ids(
        &self,
        conn: &mut C,
        refund_ids: &[Uuid],
    ) -> Result<Vec<RefundRecord>, sqlx::Error> {
        self.repo.get_refunds(conn, refund_ids).await
    }

    pub async fn get_successful_payments(
        &self,
        conn: &mut C,
        biz_id: Uuid,
    ) -> Result<Vec<PaymentRecord>, sqlx::Error> {
        self.query_payments(
            conn,
            &PaymentQuery::new()
                .biz_id(biz_id)
                .status(PaymentStatus::Success),
        )
        .await
    }

    pub async fn record_successful_payment(
        &self,
        conn: &mut C,
        req: RecordSuccessfulPaymentRequest,
    ) -> Result<(), sqlx::Error> {
        let _payment_id = self
            .repo
            .create_payment(
                conn,
                PaymentCreate {
                    description: req.description,
                    status: PaymentStatus::Success,
                    amount: req.amount,
                    biz_id: req.biz_id,
                    provider: req.provider,
                    provider_trade_no: Some(req.provider_trade_no),
                    success_at: Some(req.success_at),
                    expire_at: None,
                },
            )
            .await?;

        Ok(())
    }

    pub async fn pay(&self, conn: &mut C, key: Provider, req: PayRequest) -> PayResponse {
        let provider = self.providers.get(&key).unwrap();

        let payment_id = self
            .repo
            .create_payment(
                conn,
                PaymentCreate {
                    description: req.description.clone(),
                    status: PaymentStatus::Pending,
                    amount: req.amount,
                    biz_id: req.biz_id,
                    provider: key,
                    provider_trade_no: None,
                    success_at: None,
                    expire_at: req.expire_at,
                },
            )
            .await
            .unwrap();

        let (res, http_req, http_res) = provider.pay(payment_id, req).await;

        self.repo
            .create_payment_event(
                conn,
                PaymentEventCreate {
                    payment_id,
                    kind: PaymentEventKind::PaymentCreate,
                    http_req,
                    http_res,
                },
            )
            .await
            .unwrap();

        res
    }

    pub async fn close_payment(&self, conn: &mut C, payment_id: Uuid) {
        let payment = self
            .repo
            .query_one(conn, &PaymentQuery::new().id(payment_id))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            payment.status,
            PaymentStatus::Pending,
            "Only pending payments can be closed",
        );

        let provider = self.providers.get(&payment.provider).unwrap();

        let (http_req, http_res) = provider.close(payment_id).await;

        self.repo
            .update_payment(
                conn,
                payment_id,
                PaymentUpdate {
                    status: Some(PaymentStatus::Closed),
                    provider_trade_no: None,
                    success_at: None,
                    add_to_refunded_amount: None,
                },
            )
            .await
            .unwrap()
            .unwrap();

        self.repo
            .create_payment_event(
                conn,
                PaymentEventCreate {
                    payment_id,
                    kind: PaymentEventKind::PaymentClose,
                    http_req,
                    http_res,
                },
            )
            .await
            .unwrap();
    }

    pub async fn handle_pay_callback(
        &self,
        conn: &mut C,
        key: Provider,
        req: http::Request<bytes::Bytes>,
    ) -> (PayCallbackResult, http::Response<String>) {
        let provider = self.providers.get(&key).unwrap();

        let (outcome, http_req, http_res) = provider.pay_callback(req).await;

        let payment = self
            .repo
            .update_payment(
                conn,
                outcome.id,
                PaymentUpdate {
                    status: Some(PaymentStatus::Success),
                    provider_trade_no: Some(outcome.provider_trade_no.clone()),
                    success_at: Some(outcome.success_at),
                    add_to_refunded_amount: None,
                },
            )
            .await
            .unwrap()
            .unwrap();

        self.repo
            .create_payment_event(
                conn,
                PaymentEventCreate {
                    payment_id: outcome.id,
                    kind: PaymentEventKind::PaymentCallback,
                    http_req,
                    http_res,
                },
            )
            .await
            .unwrap();

        (
            PayCallbackResult {
                payment_id: outcome.id,
                biz_id: payment.biz_id,
                amount: payment.amount,
                status: PaymentStatus::Success,
                provider_trade_no: outcome.provider_trade_no,
                success_at: Some(outcome.success_at),
            },
            outcome.res,
        )
    }

    pub async fn handle_refund_callback(
        &self,
        conn: &mut C,
        key: Provider,
        req: http::Request<bytes::Bytes>,
    ) -> (RefundCallbackResult, http::Response<String>) {
        let provider = self.providers.get(&key).unwrap();

        let (outcome, http_req, http_res) = provider.refund_callback(req).await;

        let refund = self
            .repo
            .update_refund(
                conn,
                outcome.refund_id,
                RefundUpdate {
                    status: Some(outcome.status),
                    provider_refund_no: None,
                    success_at: outcome.success_at,
                },
            )
            .await
            .unwrap()
            .unwrap();

        if outcome.status == RefundStatus::Success {
            self.repo
                .update_payment(
                    conn,
                    refund.payment_id,
                    PaymentUpdate {
                        status: None,
                        provider_trade_no: None,
                        success_at: None,
                        add_to_refunded_amount: Some(refund.amount),
                    },
                )
                .await
                .unwrap();
        }

        self.repo
            .create_payment_event(
                conn,
                PaymentEventCreate {
                    payment_id: refund.payment_id,
                    kind: PaymentEventKind::RefundCallback,
                    http_req,
                    http_res,
                },
            )
            .await
            .unwrap();

        let payment = self
            .repo
            .query_one(conn, &PaymentQuery::new().id(refund.payment_id))
            .await
            .unwrap()
            .unwrap();

        (
            RefundCallbackResult {
                refund_id: outcome.refund_id,
                payment_id: refund.payment_id,
                biz_id: payment.biz_id,
                amount: refund.amount,
                status: outcome.status,
                provider_refund_no: refund.provider_refund_no.unwrap_or_default(),
                success_at: outcome.success_at,
            },
            outcome.res,
        )
    }

    pub async fn refund(
        &self,
        conn: &mut C,
        payment_id: Uuid,
        amount: i64,
        reason: Option<String>,
    ) -> RefundResponse {
        let payment = self
            .repo
            .query_one(conn, &PaymentQuery::new().id(payment_id))
            .await
            .unwrap()
            .unwrap();

        let provider = self.providers.get(&payment.provider).unwrap();

        let refund_id = Uuid::now_v7();

        let req = RefundRequest {
            refund_id,
            provider_trade_no: payment
                .provider_trade_no
                .expect("Payment missing provider_trade_no"),
            amount,
            total: payment.amount,
        };

        self.repo
            .create_refund(
                conn,
                RefundCreate {
                    id: refund_id,
                    payment_id,
                    amount,
                    reason,
                    status: RefundStatus::Pending,
                },
            )
            .await
            .unwrap();

        let (res, http_req, http_res) = provider.refund(payment_id, req).await;

        let status = if res.status == "SUCCESS" {
            RefundStatus::Success
        } else {
            RefundStatus::Pending
        };

        if status == RefundStatus::Success {
            self.repo
                .update_payment(
                    conn,
                    payment_id,
                    PaymentUpdate {
                        status: None,
                        provider_trade_no: None,
                        success_at: None,
                        add_to_refunded_amount: Some(amount),
                    },
                )
                .await
                .unwrap();

            self.repo
                .update_refund(
                    conn,
                    refund_id,
                    RefundUpdate {
                        status: Some(status),
                        provider_refund_no: Some(res.provider_refund_no.clone()),
                        // TODO: store in what timezone
                        success_at: Some(OffsetDateTime::now_utc()),
                    },
                )
                .await
                .unwrap();
        } else {
            self.repo
                .update_refund(
                    conn,
                    refund_id,
                    RefundUpdate {
                        status: Some(status),
                        provider_refund_no: Some(res.provider_refund_no.clone()),
                        success_at: None,
                    },
                )
                .await
                .unwrap();
        }

        self.repo
            .create_payment_event(
                conn,
                PaymentEventCreate {
                    payment_id,
                    kind: PaymentEventKind::PaymentRefund,
                    http_req,
                    http_res,
                },
            )
            .await
            .unwrap();

        res
    }
}
