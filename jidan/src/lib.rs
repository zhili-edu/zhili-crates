use serde::Serialize;
use serde_json::Value;
use sqlx::{FromRow, PgConnection};
use time::OffsetDateTime;
use uuid::Uuid;

pub mod migration;
mod query;
pub use query::OrderQuery;

#[derive(Debug, FromRow)]
pub struct OrderSummary {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,
    pub status: OrderStatus,

    pub total_items_amount: i64,
    pub payable_amount: i64,
    pub paid_amount: i64,
    pub refunded_amount: i64,

    pub created_at: OffsetDateTime,
    pub expire_at: Option<OffsetDateTime>,
}

#[derive(Debug, FromRow, Clone)]
pub struct OrderItemDetail {
    pub id: Uuid,
    pub item_id: Uuid,
    pub item_type: String,
    pub original_price: i64,
    pub unit_price: i64,
    pub real_amount: i64,
    pub extra_info: Option<Value>,
}

#[derive(Debug)]
pub struct OrderDetail {
    pub id: Uuid,
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,
    pub status: OrderStatus,
    pub total_items_amount: i64,
    pub payment_fee: i64,
    pub discount_amount: i64,
    pub payable_amount: i64,
    pub paid_amount: i64,
    pub refunded_amount: i64,
    pub refund_fee: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub expire_at: Option<OffsetDateTime>,
    pub extra_info: Option<Value>,

    pub items: Vec<OrderItemDetail>,
}

#[repr(i16)]
#[derive(Debug, Clone, Copy, Serialize)]
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

pub struct CreateOrder {
    pub user_id: Uuid,
    pub channel: String,
    pub channel_no: Option<String>,

    pub items: Vec<CreateOrderItem>,

    pub payment_fee: Option<i64>,
    pub discount_amount: Option<i64>,

    pub extra_info: Option<serde_json::Value>,
}

pub struct CreateOrderItem {
    pub item_type: String,
    pub item_id: Uuid,

    pub original_price: i64,
    pub unit_price: i64,
    pub real_amount: i64,

    pub extra_info: Option<serde_json::Value>,
}

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

#[derive(Debug, Clone)]
pub struct OrderService {
    pool: sqlx::PgPool,
}

impl OrderService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl OrderService {
    pub async fn create_order_with_id(
        &self,
        order_id: Uuid,
        info: CreateOrder,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        let total_items_amount: i64 = info.items.iter().map(|i| i.unit_price).sum();
        let payment_fee: i64 = info.payment_fee.unwrap_or(0);
        let discount_amount: i64 = info.discount_amount.unwrap_or(0);

        let payable_amount: i64 = total_items_amount + payment_fee - discount_amount;

        sqlx::query(
            r#"
            INSERT INTO jidan.orders (
                id, user_id, channel, channel_no, status,
                total_items_amount, payment_fee, discount_amount,
                payable_amount,
                extra_info
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9,
                $10
            )
            "#,
        )
        .bind(order_id)
        .bind(info.user_id)
        .bind(info.channel)
        .bind(info.channel_no)
        .bind(OrderStatus::Pending)
        .bind(total_items_amount)
        .bind(payment_fee)
        .bind(discount_amount)
        .bind(payable_amount)
        .bind(info.extra_info)
        .execute(&mut *conn)
        .await?;

        let item_type: Vec<String> = info.items.iter().map(|i| i.item_type.clone()).collect();
        let item_id: Vec<Uuid> = info.items.iter().map(|i| i.item_id).collect();
        let original_price: Vec<i64> = info.items.iter().map(|i| i.original_price).collect();
        let unit_price: Vec<i64> = info.items.iter().map(|i| i.unit_price).collect();
        let real_amount: Vec<i64> = info.items.iter().map(|i| i.real_amount).collect();
        let extra_info: Vec<Option<Value>> =
            info.items.iter().map(|i| i.extra_info.clone()).collect();

        sqlx::query(
            r#"
            WITH new_items AS (
                SELECT *
                FROM UNNEST($2, $3, $4, $5, $6, $7)
                    AS t (item_id, item_type, original_price, unit_price, real_amount, extra_info)
            )
            INSERT INTO jidan.order_items (
                order_id, item_id, item_type,
                original_price, unit_price, real_amount, extra_info
            )
            SELECT
                $1 AS order_id,
                item_id, item_type, original_price, unit_price, real_amount, extra_info
            FROM new_items
            "#,
        )
        .bind(order_id)
        .bind(item_id)
        .bind(item_type)
        .bind(original_price)
        .bind(unit_price)
        .bind(real_amount)
        .bind(extra_info)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn create_order(&self, info: CreateOrder, conn: &mut PgConnection) -> Uuid {
        let id = Uuid::now_v7();

        self.create_order_with_id(id, info, conn).await;

        id
    }

    /// 不做任何检查，将order设置为 fulfilled 状态
    pub async fn fulfill_order(
        &self,
        order_id: Uuid,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(OrderStatus::Fulfilled)
        .bind(order_id)
        .execute(conn)
        .await?;

        Ok(())
    }

    /// 记录支付金额，并将订单转换为Processing状态
    /// 如果支付金额达到或超过应付金额，自动转换为Fulfilled状态
    /// panics: 订单需在Pending或Processing状态
    pub async fn add_payment(
        &self,
        order_id: Uuid,
        payment_amount: i64,
        conn: &mut PgConnection,
    ) -> PaymentResult {
        let (current_status, current_paid_amount, payable_amount): (OrderStatus, i64, i64) =
            sqlx::query_as(
                r#"
                SELECT status as "status: OrderStatus", paid_amount, payable_amount
                FROM jidan.orders
                WHERE id = $1
                "#,
            )
            .bind(order_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();

        match current_status {
            OrderStatus::Pending | OrderStatus::Processing => {}
            _ => panic!("Order must be in pending or processing status to add payment"),
        }

        let new_paid_amount = current_paid_amount + payment_amount;

        // 自动判定状态：如果已付金额 >= 应付金额，则流转为 Fulfilled，否则为 Processing
        let new_status = if new_paid_amount >= payable_amount {
            OrderStatus::Fulfilled
        } else {
            OrderStatus::Processing
        };

        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, paid_amount = $2, updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(new_status)
        .bind(new_paid_amount)
        .bind(order_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        PaymentResult {
            order_id,
            previous_status: current_status,
            current_status: new_status,
            paid_amount: new_paid_amount,
            payable_amount,
        }
    }

    /// 记录退款金额，并根据退款情况更新订单状态
    /// 如果已退款金额 >= 已付金额，状态将更新为 Refunded
    pub async fn add_refund(
        &self,
        order_id: Uuid,
        refund_amount: i64,
        conn: &mut PgConnection,
    ) -> RefundResult {
        let (current_status, paid_amount, current_refunded_amount): (OrderStatus, i64, i64) =
            sqlx::query_as(
                r#"
                SELECT status as "status: OrderStatus", paid_amount, refunded_amount
                FROM jidan.orders
                WHERE id = $1
                "#,
            )
            .bind(order_id)
            .fetch_one(&mut *conn)
            .await
            .unwrap();

        let new_refunded_amount = current_refunded_amount + refund_amount;

        // 自动判定状态：如果已退款金额 >= 已付金额，则流转为 Refunded
        // 注意：这只是一个基础策略，具体的业务可能需要更复杂的判断
        let new_status = if new_refunded_amount >= paid_amount {
            OrderStatus::Refunded
        } else {
            current_status
        };

        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, refunded_amount = $2, updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(new_status)
        .bind(new_refunded_amount)
        .bind(order_id)
        .execute(&mut *conn)
        .await
        .unwrap();

        RefundResult {
            order_id,
            previous_status: current_status,
            current_status: new_status,
            refunded_amount: new_refunded_amount,
            paid_amount,
        }
    }

    /// 扫描并取消所有已过期的订单 (expire_at < now)
    /// 仅针对 Pending 状态的订单生效
    /// 返回修改的订单数
    pub async fn cancel_expired_orders(&self, conn: &mut PgConnection) -> u64 {
        let result = sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, updated_at = now()
            WHERE
                status = $2
                AND expire_at IS NOT NULL
                AND expire_at < now()
            "#,
        )
        .bind(OrderStatus::Canceled)
        .bind(OrderStatus::Pending)
        .execute(conn)
        .await
        .unwrap();

        result.rows_affected()
    }

    /// 将 Fulfilled 状态的订单手动标记为 Completed
    /// panics: 订单需在 Fulfilled 状态
    pub async fn complete_order(
        &self,
        order_id: Uuid,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        let current_status: OrderStatus = sqlx::query_scalar(
            r#"
            SELECT status as "status: OrderStatus"
            FROM jidan.orders
            WHERE id = $1
            "#,
        )
        .bind(order_id)
        .fetch_one(&mut *conn)
        .await?;

        match current_status {
            OrderStatus::Fulfilled => {}
            _ => panic!("Order must be in fulfilled status to be completed"),
        }

        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET status = $1, updated_at = now()
            WHERE id = $2
            "#,
        )
        .bind(OrderStatus::Completed)
        .bind(order_id)
        .execute(&mut *conn)
        .await?;

        Ok(())
    }

    pub async fn close_order(
        &self,
        order_id: Uuid,
        extra_info_patch: Option<serde_json::Value>,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        if let Some(patch) = extra_info_patch {
            sqlx::query(
                r#"
                UPDATE jidan.orders
                SET status = $1, updated_at = now(), extra_info = COALESCE(extra_info, '{}'::jsonb) || $2
                WHERE id = $3
                "#,
            )
            .bind(OrderStatus::Closed)
            .bind(patch)
            .bind(order_id)
            .execute(&mut *conn)
            .await?;
        } else {
            sqlx::query(
                r#"
                UPDATE jidan.orders
                SET status = $1, updated_at = now()
                WHERE id = $2
                "#,
            )
            .bind(OrderStatus::Closed)
            .bind(order_id)
            .execute(&mut *conn)
            .await?;
        }

        Ok(())
    }

    pub async fn update_order_extra_info(
        &self,
        order_id: Uuid,
        extra_info: serde_json::Value,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.orders
            SET updated_at = now(), extra_info = COALESCE(extra_info, '{}'::jsonb) || $1
            WHERE id = $2
            "#,
        )
        .bind(extra_info)
        .bind(order_id)
        .execute(conn)
        .await?;

        Ok(())
    }

    pub async fn update_order_item_extra_info(
        &self,
        order_item_id: Uuid,
        extra_info: serde_json::Value,
        conn: &mut PgConnection,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE jidan.order_items
            SET extra_info = COALESCE(extra_info, '{}'::jsonb) || $1
            WHERE id = $2
            "#,
        )
        .bind(extra_info)
        .bind(order_item_id)
        .execute(conn)
        .await?;

        Ok(())
    }
}
