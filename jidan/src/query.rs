use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OrderQuery<'a> {
    pub id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub status: Option<crate::OrderStatus>,
    pub channel: Option<String>,
    pub channel_no: Option<String>,
    pub created_after: Option<OffsetDateTime>,
    pub created_before: Option<OffsetDateTime>,
    pub has_skus: Option<&'a [Uuid]>,
    pub extra_info: Option<&'a Value>,
    pub item_extra_info: Option<&'a Value>,
    pub offset: i64,
    pub limit: Option<i64>,
}

impl Default for OrderQuery<'_> {
    fn default() -> Self {
        Self {
            id: None,
            user_id: None,
            status: None,
            channel: None,
            channel_no: None,
            created_after: None,
            created_before: None,
            has_skus: None,
            extra_info: None,
            item_extra_info: None,
            offset: 0,
            limit: Some(20),
        }
    }
}

impl<'a> OrderQuery<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    pub fn user_id(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn status(mut self, status: crate::OrderStatus) -> Self {
        self.status = Some(status);
        self
    }

    pub fn channel(mut self, channel: impl Into<String>) -> Self {
        self.channel = Some(channel.into());
        self
    }

    pub fn channel_no(mut self, channel_no: impl Into<String>) -> Self {
        self.channel_no = Some(channel_no.into());
        self
    }

    pub fn created_after(mut self, t: OffsetDateTime) -> Self {
        self.created_after = Some(t);
        self
    }

    pub fn created_before(mut self, t: OffsetDateTime) -> Self {
        self.created_before = Some(t);
        self
    }

    pub fn has_skus(mut self, sku_ids: &'a [Uuid]) -> Self {
        self.has_skus = Some(sku_ids);
        self
    }

    pub fn extra_info(mut self, extra_info: &'a Value) -> Self {
        self.extra_info = Some(extra_info);
        self
    }

    pub fn item_extra_info(mut self, item_extra_info: &'a Value) -> Self {
        self.item_extra_info = Some(item_extra_info);
        self
    }

    pub fn page(mut self, page: i64, page_size: i64) -> Self {
        self.offset = (page - 1).max(0) * page_size;
        self.limit = Some(page_size);
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        self.offset = offset;
        self
    }

    pub fn limit(mut self, limit: Option<i64>) -> Self {
        self.limit = limit;
        self
    }
}

impl<'a> OrderQuery<'a> {
    pub(crate) fn apply_filters(&'a self, builder: &mut sqlx::QueryBuilder<'a, sqlx::Postgres>) {
        if let Some(id) = self.id {
            builder.push(" AND id = ");
            builder.push_bind(id);
        }
        if let Some(uid) = self.user_id {
            builder.push(" AND user_id = ");
            builder.push_bind(uid);
        }
        if let Some(status) = self.status {
            builder.push(" AND status = ");
            builder.push_bind(status);
        }
        if let Some(channel) = &self.channel {
            builder.push(" AND channel = ");
            builder.push_bind(channel.as_str());
        }
        if let Some(channel_no) = &self.channel_no {
            builder.push(" AND channel_no = ");
            builder.push_bind(channel_no.as_str());
        }
        if let Some(after) = self.created_after {
            builder.push(" AND created_at >= ");
            builder.push_bind(after);
        }
        if let Some(before) = self.created_before {
            builder.push(" AND created_at < ");
            builder.push_bind(before);
        }
        if let Some(sku_ids) = &self.has_skus
            && !sku_ids.is_empty()
        {
            builder.push(" AND id IN (SELECT order_id FROM jidan.order_items WHERE sku_id = ANY(");
            builder.push_bind(sku_ids);
            builder.push("))");
        }
        if let Some(info) = self.extra_info {
            builder.push(" AND extra_info @> ");
            builder.push_bind(info);
        }
        if let Some(item_info) = self.item_extra_info {
            builder.push(" AND id IN (SELECT order_id FROM jidan.order_items WHERE extra_info @> ");
            builder.push_bind(item_info);
            builder.push(")");
        }
    }
}
