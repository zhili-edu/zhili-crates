use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct HttpRequestJson {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: serde_json::Value,
}

impl HttpRequestJson {
    pub fn from_reqwest_req(req: &reqwest::Request, body: serde_json::Value) -> Self {
        let url = req.url().to_string();

        let method = req.method().to_string();

        let headers = req
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().map(|v| (k.to_string(), v.to_string())).ok())
            .collect::<Vec<_>>();

        Self {
            url,
            method,
            headers,
            body,
        }
    }

    pub fn from_http_req(req: &http::Request<bytes::Bytes>) -> Self {
        let url = req.uri().to_string();

        let method = req.method().to_string();

        let headers = req
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().map(|v| (k.to_string(), v.to_string())).ok())
            .collect::<Vec<_>>();

        let body = serde_json::from_slice(req.body()).unwrap();

        Self {
            url,
            method,
            headers,
            body,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct HttpResponseJson {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
}

impl HttpResponseJson {
    pub fn from_http_res<T: Serialize>(res: &http::Response<T>) -> Self {
        let status = res.status().as_u16();

        let headers = res
            .headers()
            .iter()
            .filter_map(|(k, v)| v.to_str().map(|v| (k.to_string(), v.to_string())).ok())
            .collect::<Vec<_>>();

        let body = serde_json::to_value(res.body()).unwrap();

        Self {
            status,
            headers,
            body,
        }
    }
}

pub struct PaymentEvent {
    http_req: HttpRequestJson,
    http_res: Option<HttpResponseJson>,
}

#[repr(i16)]
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentEventKind {
    PaymentCreate = 0,
    PaymentCallback = 1,
    PaymentRefund = 2,
    RefundCallback = 3,
}

impl sqlx::Type<sqlx::Postgres> for PaymentEventKind {
    fn type_info() -> sqlx::postgres::PgTypeInfo {
        <i16 as sqlx::Type<sqlx::Postgres>>::type_info()
    }

    fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
        <i16 as sqlx::Type<sqlx::Postgres>>::compatible(ty)
    }
}

impl<'q> sqlx::Encode<'q, sqlx::Postgres> for PaymentEventKind {
    fn encode_by_ref(
        &self,
        buf: &mut sqlx::postgres::PgArgumentBuffer,
    ) -> Result<sqlx::encode::IsNull, Box<dyn std::error::Error + Send + Sync>> {
        let val = *self as i16;
        <i16 as sqlx::Encode<sqlx::Postgres>>::encode(val, buf)
    }
}

impl<'r> sqlx::Decode<'r, sqlx::Postgres> for PaymentEventKind {
    fn decode(
        value: sqlx::postgres::PgValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + 'static + Send + Sync>> {
        let val: i16 = <i16 as sqlx::Decode<sqlx::Postgres>>::decode(value)?;

        match val {
            0 => Ok(PaymentEventKind::PaymentCreate),
            1 => Ok(PaymentEventKind::PaymentCallback),
            2 => Ok(PaymentEventKind::PaymentRefund),
            3 => Ok(PaymentEventKind::RefundCallback),
            _ => Err(format!("Invalid PaymentStatus value: {}", val).into()),
        }
    }
}
