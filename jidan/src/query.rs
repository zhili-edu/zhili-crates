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
    pub limit: i64,
}

impl<'a> OrderQuery<'a> {
    pub fn new(limit: i64) -> Self {
        assert!(limit > 0, "limit must be > 0");

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
            limit,
        }
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
        assert!(page > 0, "page must be > 0");
        assert!(page_size > 0, "limit must be > 0");

        self.offset = (page - 1) * page_size;
        self.limit = page_size;
        self
    }

    pub fn offset(mut self, offset: i64) -> Self {
        assert!(offset >= 0, "offset must be >= 0");
        self.offset = offset;
        self
    }

    pub fn limit(mut self, limit: i64) -> Self {
        assert!(limit > 0, "limit must be > 0");
        self.limit = limit;
        self
    }

    pub(crate) fn assert_pagination_valid(&self) {
        assert!(self.offset >= 0, "offset must be >= 0");
        assert!(self.limit > 0, "limit must be > 0");
    }

    pub fn has_effective_filters(&self) -> bool {
        self.id.is_some()
            || self.user_id.is_some()
            || self.status.is_some()
            || self.channel.is_some()
            || self.channel_no.is_some()
            || self.created_after.is_some()
            || self.created_before.is_some()
            || self.has_skus.is_some_and(|sku_ids| !sku_ids.is_empty())
            || self.extra_info.is_some()
            || self.item_extra_info.is_some()
    }
}
