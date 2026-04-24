use std::{collections::HashMap, sync::Arc};

use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    PayCallbackResult, PaymentRecord, PaymentStatus, Provider, RecordCashPaymentRequest,
    RecordCashRefundRequest, RecordSuccessfulPaymentRequest, RefundCallbackResult, RefundRecord,
    RefundStatus,
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

fn empty_provider_info() -> serde_json::Value {
    serde_json::json!({})
}

fn cash_payment_provider_info(
    operator_id: Option<Uuid>,
    note: Option<String>,
) -> serde_json::Value {
    let mut provider_info = serde_json::Map::new();

    if let Some(operator_id) = operator_id {
        provider_info.insert("operator_id".to_string(), serde_json::json!(operator_id));
    }

    if let Some(note) = note {
        provider_info.insert("note".to_string(), serde_json::json!(note));
    }

    serde_json::Value::Object(provider_info)
}

impl<C> Clone for PaymentService<C> {
    fn clone(&self) -> Self {
        Self {
            providers: Arc::clone(&self.providers),
            repo: Arc::clone(&self.repo),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use async_trait::async_trait;
    use time::OffsetDateTime;
    use uuid::Uuid;

    use super::PaymentService;
    use crate::{
        PayCallbackResult, PaymentRecord, PaymentStatus, Provider, RecordCashPaymentRequest,
        RecordCashRefundRequest, RefundCallbackResult, RefundRecord, RefundStatus,
        psp::PaymentServiceProvider,
        repo::{
            PaymentCreate, PaymentEventCreate, PaymentQuery, PaymentRepository, PaymentUpdate,
            RefundCreate, RefundUpdate,
        },
    };

    #[derive(Debug, Clone)]
    struct CapturedRefund {
        id: Uuid,
        payment_id: Uuid,
        provider_refund_no: Option<String>,
        amount: i64,
        reason: Option<String>,
        status: RefundStatus,
        success_at: Option<OffsetDateTime>,
    }

    #[derive(Default)]
    struct FakePaymentRepo {
        payments: Mutex<HashMap<Uuid, PaymentRecord>>,
        refunds: Mutex<HashMap<Uuid, CapturedRefund>>,
        event_count: AtomicUsize,
    }

    impl FakePaymentRepo {
        fn payment(&self, id: Uuid) -> PaymentRecord {
            self.payments.lock().unwrap().get(&id).unwrap().clone()
        }

        fn refund(&self, id: Uuid) -> CapturedRefund {
            self.refunds.lock().unwrap().get(&id).unwrap().clone()
        }

        fn event_count(&self) -> usize {
            self.event_count.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PaymentRepository for FakePaymentRepo {
        type Context = ();

        async fn query(
            &self,
            _conn: &mut Self::Context,
            _query: &PaymentQuery,
        ) -> Result<Vec<PaymentRecord>, sqlx::Error> {
            Ok(self.payments.lock().unwrap().values().cloned().collect())
        }

        async fn query_one(
            &self,
            _conn: &mut Self::Context,
            _query: &PaymentQuery,
        ) -> Result<Option<PaymentRecord>, sqlx::Error> {
            Ok(self.payments.lock().unwrap().values().next().cloned())
        }

        async fn query_for_update_skip_locked(
            &self,
            conn: &mut Self::Context,
            query: &PaymentQuery,
        ) -> Result<Option<PaymentRecord>, sqlx::Error> {
            self.query_one(conn, query).await
        }

        async fn create_payment(
            &self,
            _conn: &mut Self::Context,
            info: PaymentCreate,
        ) -> Result<Uuid, sqlx::Error> {
            let id = Uuid::now_v7();
            let payment = PaymentRecord {
                id,
                provider_trade_no: info.provider_trade_no,
                amount: info.amount,
                refunded_amount: 0,
                biz_id: info.biz_id,
                provider: info.provider,
                provider_info: info.provider_info,
                status: info.status,
                success_at: info.success_at,
                expire_at: info.expire_at,
            };

            self.payments.lock().unwrap().insert(id, payment);

            Ok(id)
        }

        async fn update_payment(
            &self,
            _conn: &mut Self::Context,
            id: Uuid,
            info: PaymentUpdate,
        ) -> Result<Option<PaymentRecord>, sqlx::Error> {
            let mut payments = self.payments.lock().unwrap();
            let Some(payment) = payments.get_mut(&id) else {
                return Ok(None);
            };

            if let Some(status) = info.status {
                payment.status = status;
            }
            if let Some(provider_trade_no) = info.provider_trade_no {
                payment.provider_trade_no = Some(provider_trade_no);
            }
            if let Some(success_at) = info.success_at {
                payment.success_at = Some(success_at);
            }
            if let Some(add_to_refunded_amount) = info.add_to_refunded_amount {
                payment.refunded_amount += add_to_refunded_amount;
            }

            Ok(Some(payment.clone()))
        }

        async fn create_payment_event(
            &self,
            _conn: &mut Self::Context,
            _info: PaymentEventCreate,
        ) -> Result<Uuid, sqlx::Error> {
            self.event_count.fetch_add(1, Ordering::SeqCst);
            Ok(Uuid::now_v7())
        }

        async fn get_refunds(
            &self,
            _conn: &mut Self::Context,
            ids: &[Uuid],
        ) -> Result<Vec<RefundRecord>, sqlx::Error> {
            let refunds = self.refunds.lock().unwrap();
            Ok(ids
                .iter()
                .filter_map(|id| refunds.get(id))
                .map(|refund| RefundRecord {
                    id: refund.id,
                    provider_refund_no: refund.provider_refund_no.clone(),
                    payment_id: refund.payment_id,
                    amount: refund.amount,
                })
                .collect())
        }

        async fn create_refund(
            &self,
            _conn: &mut Self::Context,
            info: RefundCreate,
        ) -> Result<Uuid, sqlx::Error> {
            let refund = CapturedRefund {
                id: info.id,
                payment_id: info.payment_id,
                provider_refund_no: info.provider_refund_no,
                amount: info.amount,
                reason: info.reason,
                status: info.status,
                success_at: info.success_at,
            };

            self.refunds.lock().unwrap().insert(info.id, refund);

            Ok(info.id)
        }

        async fn update_refund(
            &self,
            _conn: &mut Self::Context,
            id: Uuid,
            info: RefundUpdate,
        ) -> Result<Option<RefundRecord>, sqlx::Error> {
            let mut refunds = self.refunds.lock().unwrap();
            let Some(refund) = refunds.get_mut(&id) else {
                return Ok(None);
            };

            if let Some(status) = info.status {
                refund.status = status;
            }
            if let Some(provider_refund_no) = info.provider_refund_no {
                refund.provider_refund_no = Some(provider_refund_no);
            }
            if let Some(success_at) = info.success_at {
                refund.success_at = Some(success_at);
            }

            Ok(Some(RefundRecord {
                id: refund.id,
                provider_refund_no: refund.provider_refund_no.clone(),
                payment_id: refund.payment_id,
                amount: refund.amount,
            }))
        }
    }

    fn service_with_repo(repo: Arc<FakePaymentRepo>) -> PaymentService<()> {
        let providers: HashMap<Provider, Box<dyn PaymentServiceProvider + Send + Sync>> =
            HashMap::new();

        PaymentService {
            providers: Arc::new(providers),
            repo,
        }
    }

    fn ts(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(seconds).unwrap()
    }

    #[tokio::test]
    async fn record_cash_payment_creates_success_payment_without_event() {
        let repo = Arc::new(FakePaymentRepo::default());
        let service = service_with_repo(Arc::clone(&repo));
        let mut conn = ();
        let biz_id = Uuid::now_v7();
        let operator_id = Uuid::now_v7();
        let collected_at = ts(1_700_000_000);

        let PayCallbackResult {
            payment_id,
            biz_id: result_biz_id,
            amount,
            status,
            provider_trade_no,
            success_at,
        } = service
            .record_cash_payment(
                &mut conn,
                RecordCashPaymentRequest {
                    description: "cash order".to_string(),
                    amount: 1200,
                    biz_id,
                    receipt_no: Some("receipt-1".to_string()),
                    collected_at,
                    operator_id: Some(operator_id),
                    note: Some("front desk".to_string()),
                },
            )
            .await
            .unwrap();

        let payment = repo.payment(payment_id);
        let operator_id_json = serde_json::json!(operator_id);
        let note_json = serde_json::json!("front desk");

        assert_eq!(result_biz_id, biz_id);
        assert_eq!(amount, 1200);
        assert_eq!(status, PaymentStatus::Success);
        assert_eq!(provider_trade_no, "receipt-1");
        assert_eq!(success_at, Some(collected_at));
        assert_eq!(payment.provider, Provider::Cash);
        assert_eq!(payment.status, PaymentStatus::Success);
        assert_eq!(payment.provider_trade_no.as_deref(), Some("receipt-1"));
        assert_eq!(
            payment.provider_info.get("operator_id"),
            Some(&operator_id_json)
        );
        assert_eq!(payment.provider_info.get("note"), Some(&note_json));
        assert_eq!(repo.event_count(), 0);
    }

    #[tokio::test]
    async fn record_cash_refund_creates_success_refund_without_event() {
        let repo = Arc::new(FakePaymentRepo::default());
        let service = service_with_repo(Arc::clone(&repo));
        let mut conn = ();
        let payment = service
            .record_cash_payment(
                &mut conn,
                RecordCashPaymentRequest {
                    description: "cash order".to_string(),
                    amount: 1200,
                    biz_id: Uuid::now_v7(),
                    receipt_no: Some("receipt-1".to_string()),
                    collected_at: ts(1_700_000_000),
                    operator_id: None,
                    note: None,
                },
            )
            .await
            .unwrap();

        let refunded_at = ts(1_700_000_100);

        let RefundCallbackResult {
            refund_id,
            payment_id,
            amount,
            status,
            provider_refund_no,
            success_at,
            ..
        } = service
            .record_cash_refund(
                &mut conn,
                RecordCashRefundRequest {
                    payment_id: payment.payment_id,
                    amount: 500,
                    reason: Some("customer return".to_string()),
                    refund_no: Some("refund-1".to_string()),
                    refunded_at,
                },
            )
            .await
            .unwrap();

        let payment = repo.payment(payment_id);
        let refund = repo.refund(refund_id);

        assert_eq!(amount, 500);
        assert_eq!(status, RefundStatus::Success);
        assert_eq!(provider_refund_no, "refund-1");
        assert_eq!(success_at, Some(refunded_at));
        assert_eq!(payment.refunded_amount, 500);
        assert_eq!(refund.status, RefundStatus::Success);
        assert_eq!(refund.reason.as_deref(), Some("customer return"));
        assert_eq!(refund.provider_refund_no.as_deref(), Some("refund-1"));
        assert_eq!(refund.success_at, Some(refunded_at));
        assert_eq!(repo.event_count(), 0);
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
                    provider_info: empty_provider_info(),
                    provider_trade_no: Some(req.provider_trade_no),
                    success_at: Some(req.success_at),
                    expire_at: None,
                },
            )
            .await?;

        Ok(())
    }

    pub async fn record_cash_payment(
        &self,
        conn: &mut C,
        req: RecordCashPaymentRequest,
    ) -> Result<PayCallbackResult, sqlx::Error> {
        assert!(req.amount > 0, "cash payment amount must be positive");

        let RecordCashPaymentRequest {
            description,
            amount,
            biz_id,
            receipt_no,
            collected_at,
            operator_id,
            note,
        } = req;

        let provider_info = cash_payment_provider_info(operator_id, note);

        let payment_id = self
            .repo
            .create_payment(
                conn,
                PaymentCreate {
                    description,
                    status: PaymentStatus::Success,
                    amount,
                    biz_id,
                    provider: Provider::Cash,
                    provider_info,
                    provider_trade_no: receipt_no.clone(),
                    success_at: Some(collected_at),
                    expire_at: None,
                },
            )
            .await?;

        Ok(PayCallbackResult {
            payment_id,
            biz_id,
            amount,
            status: PaymentStatus::Success,
            provider_trade_no: receipt_no.unwrap_or_default(),
            success_at: Some(collected_at),
        })
    }

    pub async fn pay(&self, conn: &mut C, key: Provider, req: PayRequest) -> PayResponse {
        assert_ne!(
            key,
            Provider::Cash,
            "cash payments must be recorded with record_cash_payment",
        );

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
                    provider_info: empty_provider_info(),
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

        assert_ne!(
            payment.provider,
            Provider::Cash,
            "cash payments cannot be closed",
        );

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
        assert_ne!(
            key,
            Provider::Cash,
            "cash payments do not have provider callbacks",
        );

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
        assert_ne!(
            key,
            Provider::Cash,
            "cash refunds do not have provider callbacks",
        );

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

        assert_ne!(
            payment.provider,
            Provider::Cash,
            "cash refunds must be recorded with record_cash_refund",
        );

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
                    provider_refund_no: None,
                    success_at: None,
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

    pub async fn record_cash_refund(
        &self,
        conn: &mut C,
        req: RecordCashRefundRequest,
    ) -> Result<RefundCallbackResult, sqlx::Error> {
        assert!(req.amount > 0, "cash refund amount must be positive");

        let RecordCashRefundRequest {
            payment_id,
            amount,
            reason,
            refund_no,
            refunded_at,
        } = req;

        let payment = self
            .repo
            .query_one(conn, &PaymentQuery::new().id(payment_id))
            .await?
            .expect("Payment not found");

        assert_eq!(
            payment.provider,
            Provider::Cash,
            "only cash payments can be refunded with record_cash_refund",
        );

        assert!(
            payment.refunded_amount + amount <= payment.amount,
            "cash refund amount exceeds payment amount",
        );

        let refund_id = Uuid::now_v7();

        self.repo
            .create_refund(
                conn,
                RefundCreate {
                    id: refund_id,
                    payment_id,
                    amount,
                    reason,
                    status: RefundStatus::Success,
                    provider_refund_no: refund_no.clone(),
                    success_at: Some(refunded_at),
                },
            )
            .await?;

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
            .await?;

        Ok(RefundCallbackResult {
            refund_id,
            payment_id,
            biz_id: payment.biz_id,
            amount,
            status: RefundStatus::Success,
            provider_refund_no: refund_no.unwrap_or_default(),
            success_at: Some(refunded_at),
        })
    }
}
