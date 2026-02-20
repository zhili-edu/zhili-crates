use async_trait::async_trait;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{
    PaymentRecord, PaymentStatus, Provider, RefundRecord, RefundStatus,
    event::{HttpRequestJson, HttpResponseJson, PaymentEventKind},
};

#[derive(Debug, Default)]
pub struct PaymentQuery {
    id: Option<Uuid>,
    biz_id: Option<Uuid>,
    status: Option<PaymentStatus>,
}

impl PaymentQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    pub fn biz_id(mut self, biz_id: Uuid) -> Self {
        self.biz_id = Some(biz_id);
        self
    }

    pub fn status(mut self, status: PaymentStatus) -> Self {
        self.status = Some(status);
        self
    }
}

impl PaymentQuery {
    pub(crate) fn apply_filters(&self, builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>) {
        if let Some(id) = self.id {
            builder.push(" AND id = ");
            builder.push_bind(id);
        }
        if let Some(biz_id) = self.biz_id {
            builder.push(" AND biz_id = ");
            builder.push_bind(biz_id);
        }
        if let Some(status) = self.status {
            builder.push(" AND status = ");
            builder.push_bind(status);
        }
    }
}

pub struct PaymentCreate {
    pub description: String,
    pub status: PaymentStatus,
    pub amount: i64,
    pub biz_id: Uuid,
    pub provider: Provider,
    pub provider_trade_no: Option<String>,
    pub success_at: Option<time::OffsetDateTime>,
    pub expire_at: Option<time::OffsetDateTime>,
}

pub struct PaymentUpdate {
    pub status: Option<PaymentStatus>,
    pub provider_trade_no: Option<String>,
    pub success_at: Option<time::OffsetDateTime>,
    pub add_to_refunded_amount: Option<i64>,
}

pub struct PaymentEventCreate {
    pub payment_id: Uuid,
    pub kind: PaymentEventKind,
    pub http_req: HttpRequestJson,
    pub http_res: Option<HttpResponseJson>,
}

pub struct RefundCreate {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub amount: i64,
    pub reason: Option<String>,
    pub status: RefundStatus,
}

pub struct RefundUpdate {
    pub status: Option<RefundStatus>,
    pub provider_refund_no: Option<String>,
    pub success_at: Option<time::OffsetDateTime>,
}

#[async_trait]
pub trait PaymentRepository: Send + Sync {
    type Context;

    async fn query_payments(
        &self,
        conn: &mut Self::Context,
        query: &PaymentQuery,
    ) -> Result<Vec<PaymentRecord>, sqlx::Error>;

    async fn query_payment_optional(
        &self,
        conn: &mut Self::Context,
        query: &PaymentQuery,
    ) -> Result<Option<PaymentRecord>, sqlx::Error>;

    async fn create_payment(
        &self,
        conn: &mut Self::Context,
        info: PaymentCreate,
    ) -> Result<Uuid, sqlx::Error>;

    async fn update_payment(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        info: PaymentUpdate,
    ) -> Result<Option<PaymentRecord>, sqlx::Error>;

    async fn create_payment_event(
        &self,
        conn: &mut Self::Context,
        info: PaymentEventCreate,
    ) -> Result<Uuid, sqlx::Error>;

    async fn get_refunds(
        &self,
        conn: &mut Self::Context,
        ids: &[Uuid],
    ) -> Result<Vec<RefundRecord>, sqlx::Error>;

    async fn create_refund(
        &self,
        conn: &mut Self::Context,
        info: RefundCreate,
    ) -> Result<Uuid, sqlx::Error>;

    async fn update_refund(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        info: RefundUpdate,
    ) -> Result<Option<RefundRecord>, sqlx::Error>;
}

pub struct PaymentRepo;

#[async_trait]
impl PaymentRepository for PaymentRepo {
    type Context = PgConnection;

    async fn query_payments(
        &self,
        conn: &mut Self::Context,
        query: &PaymentQuery,
    ) -> Result<Vec<PaymentRecord>, sqlx::Error> {
        let mut builder = sqlx::QueryBuilder::<'_, sqlx::Postgres>::new(
            r#"
            SELECT
                id, provider_trade_no, amount, refunded_amount,
                biz_id, provider, status, success_at, expire_at
            FROM bokchoy.payments
            WHERE 1=1
            "#,
        );

        query.apply_filters(&mut builder);

        builder.push(" ORDER BY created_at DESC");

        builder
            .build_query_as::<PaymentRecord>()
            .fetch_all(conn)
            .await
    }

    async fn query_payment_optional(
        &self,
        conn: &mut Self::Context,
        query: &PaymentQuery,
    ) -> Result<Option<PaymentRecord>, sqlx::Error> {
        let mut builder = sqlx::QueryBuilder::<'_, sqlx::Postgres>::new(
            r#"
            SELECT
                id, provider_trade_no, amount, refunded_amount,
                biz_id, provider, status, success_at, expire_at
            FROM bokchoy.payments
            WHERE 1=1
            "#,
        );

        query.apply_filters(&mut builder);

        builder.push(" ORDER BY created_at DESC");
        builder.push(" LIMIT 1");

        builder
            .build_query_as::<PaymentRecord>()
            .fetch_optional(conn)
            .await
    }

    async fn create_payment(
        &self,
        conn: &mut Self::Context,
        info: PaymentCreate,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<sqlx::Postgres, Uuid>(
            r#"
            INSERT INTO bokchoy.payments (
                provider_trade_no, description, status, amount, refunded_amount,
                biz_id, provider,
                created_at, updated_at, success_at, expire_at
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7,
                now(), now(), $8, $9
            )
            RETURNING id
            "#,
        )
        .bind(info.provider_trade_no)
        .bind(info.description)
        .bind(info.status)
        .bind(info.amount)
        .bind(0)
        .bind(info.biz_id)
        .bind(info.provider)
        .bind(info.success_at)
        .bind(info.expire_at)
        .fetch_one(conn)
        .await
    }

    async fn update_payment(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        info: PaymentUpdate,
    ) -> Result<Option<PaymentRecord>, sqlx::Error> {
        sqlx::query_as::<sqlx::Postgres, PaymentRecord>(
            r#"
            UPDATE bokchoy.payments
            SET
                status = coalesce($2, status),
                provider_trade_no = coalesce($3, provider_trade_no),
                success_at = coalesce($4, success_at),
                refunded_amount = coalesce($5, 0) + refunded_amount,
                updated_at = now()
            WHERE id = $1
            RETURNING
                id, provider_trade_no, amount, refunded_amount,
                biz_id, provider, status, success_at, expire_at
            "#,
        )
        .bind(id)
        .bind(info.status)
        .bind(info.provider_trade_no)
        .bind(info.success_at)
        .bind(info.add_to_refunded_amount)
        .fetch_optional(conn)
        .await
    }

    async fn create_payment_event(
        &self,
        conn: &mut Self::Context,
        info: PaymentEventCreate,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<sqlx::Postgres, Uuid>(
            r#"
            INSERT INTO bokchoy.payment_events (
                payment_id, kind, http_req, http_res, created_at
            )
            VALUES (
                $1, $2, $3, $4, now()
            )
            RETURNING id
            "#,
        )
        .bind(info.payment_id)
        .bind(info.kind)
        .bind(serde_json::to_value(info.http_req).unwrap())
        .bind(serde_json::to_value(info.http_res).unwrap())
        .fetch_one(conn)
        .await
    }

    async fn get_refunds(
        &self,
        conn: &mut Self::Context,
        ids: &[Uuid],
    ) -> Result<Vec<RefundRecord>, sqlx::Error> {
        sqlx::query_as::<sqlx::Postgres, RefundRecord>(
            r#"
            SELECT
                id, provider_refund_no, payment_id, amount
            FROM bokchoy.refunds
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_all(conn)
        .await
    }

    async fn create_refund(
        &self,
        conn: &mut Self::Context,
        info: RefundCreate,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar::<sqlx::Postgres, Uuid>(
            r#"
            INSERT INTO bokchoy.refunds (
                id, payment_id,
                amount, reason, status,
                created_at, updated_at
            )
            VALUES (
                $1, $2,
                $3, $4, $5,
                now(), now()
            )
            RETURNING id
            "#,
        )
        .bind(info.id)
        .bind(info.payment_id)
        .bind(info.amount)
        .bind(info.reason)
        .bind(info.status)
        .fetch_one(conn)
        .await
    }

    async fn update_refund(
        &self,
        conn: &mut Self::Context,
        id: Uuid,
        info: RefundUpdate,
    ) -> Result<Option<RefundRecord>, sqlx::Error> {
        sqlx::query_as::<sqlx::Postgres, RefundRecord>(
            r#"
            UPDATE bokchoy.refunds
            SET
                status = coalesce($2, status),
                provider_refund_no = coalesce($3, provider_refund_no),
                success_at = coalesce($4, success_at),
                updated_at = now()
            WHERE id = $1
            RETURNING id, provider_refund_no, payment_id, amount
            "#,
        )
        .bind(id)
        .bind(info.status)
        .bind(info.provider_refund_no)
        .bind(info.success_at)
        .fetch_optional(conn)
        .await
    }
}
