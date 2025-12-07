use aes_gcm::{
    Aes256Gcm, Key, KeyInit as _, Nonce,
    aead::{Aead, Payload},
};
use http::HeaderValue;
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    RefundStatus,
    event::{HttpRequestJson, HttpResponseJson},
    psp::{
        PayCallbackOutcome, PayRequest, PayResponse, PaymentServiceProvider, RefundCallbackOutcome,
        RefundRequest, RefundResponse,
    },
    utils::{get_body_auth_header, pay_sign, verify_response},
};

#[derive(Debug)]
pub struct WxPayJsapi {
    appid: String,
    mchid: String,
    payment_notify_url: String,
    refund_notify_url: String,
    merchant_cert_serial_no: String,
    merchant_cert_private_key: rsa::RsaPrivateKey,
    wxpay_public_key_id: String,
    wxpay_public_key: rsa::RsaPublicKey,
    apiv3_key: String,
    reqwest: reqwest::Client,
}

impl WxPayJsapi {
    pub fn new(
        (appid, mchid): (String, String),
        payment_notify_url: String,
        refund_notify_url: String,
        merchant_cert_serial_no: String,
        merchant_cert_private_key: rsa::RsaPrivateKey,
        wxpay_public_key_id: String,
        wxpay_public_key: rsa::RsaPublicKey,
        apiv3_key: String,
    ) -> Self {
        let reqwest = reqwest::Client::new();

        Self {
            appid,
            mchid,
            payment_notify_url,
            refund_notify_url,
            merchant_cert_serial_no,
            merchant_cert_private_key,
            wxpay_public_key_id,
            wxpay_public_key,
            apiv3_key,
            reqwest,
        }
    }
}

#[async_trait::async_trait]
impl PaymentServiceProvider for WxPayJsapi {
    async fn pay(
        &self,
        id: Uuid,
        mut req: PayRequest,
    ) -> (PayResponse, HttpRequestJson, Option<HttpResponseJson>) {
        const API_PATH: &str = "/v3/pay/transactions/jsapi";

        let payer_openid = req.extras.remove("openid").unwrap();

        let body = json!({
            "appid": self.appid,
            "mchid": self.mchid,
            "description": req.description,
            "out_trade_no": id.simple().to_string(),
            "notify_url": self.payment_notify_url,
            "amount": { "total": req.amount, "currency": "CNY" },
            "payer": { "openid": payer_openid }
        });

        let body_str = serde_json::to_string(&body).unwrap();

        let auth_header = get_body_auth_header(
            &self.mchid,
            self.merchant_cert_private_key.clone(),
            &self.merchant_cert_serial_no,
            http::Method::POST,
            API_PATH,
            &body_str,
        );

        let req = self
            .reqwest
            .post(format!("https://api.mch.weixin.qq.com{API_PATH}"))
            .body(body_str)
            .header("Authorization", auth_header)
            .header("User-Agent", "bokchoy")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .build()
            .unwrap();

        let http_req = HttpRequestJson::from_reqwest_req(&req, body);

        let res = self.reqwest.execute(req).await.unwrap();

        if res.status().is_success() {
            #[derive(Deserialize)]
            struct PrePayResponse {
                prepay_id: String,
            }

            let (http_res, body) = {
                let status = res.status().as_u16();

                let headers = res
                    .headers()
                    .iter()
                    .filter_map(|(k, v)| v.to_str().map(|v| (k.to_string(), v.to_string())).ok())
                    .collect::<Vec<_>>();

                let body = res.json::<serde_json::Value>().await.unwrap();

                (
                    HttpResponseJson {
                        status,
                        headers,
                        body: body.clone(),
                    },
                    body,
                )
            };

            let body = serde_json::from_value::<PrePayResponse>(body).unwrap();

            let (timestamp, nonce, sign) = pay_sign(
                &self.appid,
                self.merchant_cert_private_key.clone(),
                &body.prepay_id,
            );

            let params = json!({
                "timeStamp": timestamp.to_string(),
                "nonceStr": nonce,
                "package": format!("prepay_id={}", body.prepay_id),
                "signType": "RSA",
                "paySign": sign,
            });

            return (
                PayResponse {
                    provider_params: params,
                },
                http_req,
                Some(http_res),
            );
        } else {
            println!("{:?}", res.text().await);
            todo!();
        }
    }

    async fn pay_callback(
        &self,
        req: http::Request<bytes::Bytes>,
    ) -> (
        PayCallbackOutcome,
        HttpRequestJson,
        Option<HttpResponseJson>,
    ) {
        let http_req = HttpRequestJson::from_http_req(&req);

        let header = req.headers();

        let Some(Ok(timestamp)) = header.get("Wechatpay-Timestamp").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(nonce)) = header.get("Wechatpay-Nonce").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(cert_serial)) = header.get("Wechatpay-Serial").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(sign)) = header.get("Wechatpay-Signature").map(HeaderValue::to_str) else {
            panic!();
        };

        if cert_serial != self.wxpay_public_key_id {
            // TODO: 平台证书
            panic!();
        }

        if verify_response(
            self.wxpay_public_key.clone(),
            sign,
            timestamp,
            nonce,
            req.body(),
        )
        .is_err()
        {
            panic!();
        }

        let Ok(body) = serde_json::from_slice::<serde_json::Value>(req.body()) else {
            panic!();
        };

        if body["event_type"] != "TRANSACTION.SUCCESS" {
            panic!();
        }

        if body["resource_type"] != "encrypt-resource" {
            panic!();
        }

        let encrypted =
            serde_json::from_value::<EncryptedResource>(body["resource"].clone()).unwrap();

        let plain_text = encrypted.decrypt(self.apiv3_key.as_bytes()).unwrap();

        let resource = serde_json::from_str::<PlainResource>(&plain_text).unwrap();

        if resource.appid != self.appid || resource.mchid != self.mchid {
            panic!();
        }

        if resource.trade_state != "SUCCESS" {
            panic!();
        }

        let res = http::Response::builder()
            .header("Content-Type", "application/json")
            .body(json!({}).to_string())
            .unwrap();

        let http_res = HttpResponseJson::from_http_res(&res);

        (
            PayCallbackOutcome {
                id: resource.out_trade_no,
                provider_trade_no: resource.transaction_id,
                success_at: resource.success_time,
                res,
            },
            http_req,
            Some(http_res),
        )
    }

    async fn refund(
        &self,
        payment_id: Uuid,
        req: RefundRequest,
    ) -> (RefundResponse, HttpRequestJson, Option<HttpResponseJson>) {
        const API_PATH: &str = "/v3/refund/domestic/refunds";

        let body = json!({
            "out_trade_no": payment_id.simple().to_string(),
            "out_refund_no": req.refund_id.simple().to_string(),
            "notify_url": self.refund_notify_url,
            "amount": {
                "refund": req.amount,
                "total": req.total,
                "currency": "CNY"
            }
        });

        let body_str = serde_json::to_string(&body).unwrap();

        let auth_header = get_body_auth_header(
            &self.mchid,
            self.merchant_cert_private_key.clone(),
            &self.merchant_cert_serial_no,
            http::Method::POST,
            API_PATH,
            &body_str,
        );

        let req_http = self
            .reqwest
            .post(format!("https://api.mch.weixin.qq.com{API_PATH}"))
            .body(body_str.clone())
            .header("Authorization", auth_header)
            .header("User-Agent", "bokchoy")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .build()
            .unwrap();

        let http_req = HttpRequestJson::from_reqwest_req(&req_http, body);

        let res = self.reqwest.execute(req_http).await.unwrap();

        let (http_res, body_json) = {
            let status = res.status().as_u16();

            let headers = res
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().map(|v| (k.to_string(), v.to_string())).ok())
                .collect::<Vec<_>>();

            let body = res.json::<serde_json::Value>().await.unwrap();

            (
                HttpResponseJson {
                    status,
                    headers,
                    body: body.clone(),
                },
                body,
            )
        };

        if http_res.status >= 200 && http_res.status < 300 {
            let refund_no = body_json["refund_id"].as_str().unwrap().to_string();
            let status = body_json["status"].as_str().unwrap().to_string();

            (
                RefundResponse {
                    refund_id: req.refund_id,
                    provider_refund_no: refund_no,
                    status,
                },
                http_req,
                Some(http_res),
            )
        } else {
            // Log the error and panic as per instructions to ignore robust error handling for now
            panic!(
                "Refund failed: status={}, body={}",
                http_res.status, body_json
            );
        }
    }

    async fn refund_callback(
        &self,
        req: http::Request<bytes::Bytes>,
    ) -> (
        RefundCallbackOutcome,
        HttpRequestJson,
        Option<HttpResponseJson>,
    ) {
        let http_req = HttpRequestJson::from_http_req(&req);

        let header = req.headers();

        let Some(Ok(timestamp)) = header.get("Wechatpay-Timestamp").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(nonce)) = header.get("Wechatpay-Nonce").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(cert_serial)) = header.get("Wechatpay-Serial").map(HeaderValue::to_str) else {
            panic!();
        };
        let Some(Ok(sign)) = header.get("Wechatpay-Signature").map(HeaderValue::to_str) else {
            panic!();
        };

        if cert_serial != self.wxpay_public_key_id {
            // TODO: 平台证书
            panic!();
        }

        if verify_response(
            self.wxpay_public_key.clone(),
            sign,
            timestamp,
            nonce,
            req.body(),
        )
        .is_err()
        {
            panic!();
        }

        let Ok(body) = serde_json::from_slice::<serde_json::Value>(req.body()) else {
            panic!();
        };

        if body["resource_type"] != "encrypt-resource" {
            panic!();
        }

        let encrypted =
            serde_json::from_value::<EncryptedResource>(body["resource"].clone()).unwrap();

        let plain_text = encrypted.decrypt(self.apiv3_key.as_bytes()).unwrap();

        let resource = serde_json::from_str::<PlainRefundResource>(&plain_text).unwrap();

        if resource.mchid != self.mchid {
            panic!();
        }

        let status = match resource.refund_status.as_str() {
            "SUCCESS" => RefundStatus::Success,
            "CLOSED" | "ABNORMAL" => RefundStatus::Failed, // Simplified mapping
            _ => RefundStatus::Pending,                    // Should not happen in callback usually
        };

        let res = http::Response::builder()
            .header("Content-Type", "application/json")
            .body(json!({}).to_string())
            .unwrap();

        let http_res = HttpResponseJson::from_http_res(&res);

        (
            RefundCallbackOutcome {
                refund_id: resource.out_refund_no,
                provider_refund_no: resource.refund_id,
                success_at: resource.success_time,
                status,
                res,
            },
            http_req,
            Some(http_res),
        )
    }
}

#[derive(Deserialize, Debug)]
pub struct EncryptedResource {
    pub ciphertext: String,
    pub nonce: String,
    #[serde(default)]
    pub associated_data: String,
}

impl EncryptedResource {
    pub fn decrypt(&self, key: &[u8]) -> Result<String, ()> {
        let key = Key::<Aes256Gcm>::from_slice(key);
        let cipher = Aes256Gcm::new(key);

        let nonce = Nonce::from_slice(self.nonce.as_bytes());

        let msg = {
            use base64::prelude::*;

            BASE64_STANDARD.decode(&self.ciphertext).map_err(|_| ())?
        };

        let payload = Payload {
            msg: &msg,
            aad: self.associated_data.as_bytes(),
        };

        let data = cipher.decrypt(nonce, payload).map_err(|_| ())?;

        String::from_utf8(data).map_err(|_| ())
    }
}

#[derive(Deserialize)]
struct PlainResource {
    appid: String,
    mchid: String,
    out_trade_no: Uuid,
    transaction_id: String,
    trade_state: String,
    #[serde(with = "time::serde::rfc3339")]
    success_time: OffsetDateTime,
}

#[derive(Deserialize)]
struct PlainRefundResource {
    mchid: String,
    out_refund_no: Uuid,
    refund_id: String,
    refund_status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    success_time: Option<OffsetDateTime>,
}
