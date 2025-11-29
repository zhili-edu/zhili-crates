use http::HeaderValue;
use regex::{Captures, Regex};
use rsa::{RsaPublicKey, pkcs1v15, pkcs8::DecodePublicKey as _, signature::Verifier as _};
use s3::{PostPolicy, PostPolicyField, PostPolicyValue};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::Kufang;

#[derive(Debug, Serialize)]
pub struct PostObjectUrl {
    pub url: String,
    pub fields: Vec<(String, String)>,
}

impl Kufang {
    pub async fn get_post_object_url(&self, callback_url: &str, public: bool) -> PostObjectUrl {
        let file_id = Uuid::now_v7();
        let object_key = self.get_s3_key(file_id, public);

        let callback = get_post_object_callback(
            callback_url,
            Some(vec![
                ("file_id", file_id.to_string().into()),
                ("object_key", "${object}".into()),
                ("mime", "${mimeType}".into()),
                ("size", "${size}".into()),
                ("md5_base64", "${contentMd5}".into()),
                ("public", public.into()),
            ]),
        );

        let policy = PostPolicy::new(30)
            .condition(
                PostPolicyField::Bucket,
                PostPolicyValue::Exact(self.bucket.name.as_str().into()),
            )
            .expect("bucket policy")
            .condition(
                PostPolicyField::Custom("callback".into()),
                PostPolicyValue::Exact(callback.into()),
            )
            .expect("bucket policy")
            .condition(
                PostPolicyField::ContentLengthRange,
                PostPolicyValue::Range(0, 100 * 1024 * 1024), // 100M
            )
            .expect("bucket policy")
            .condition(
                PostPolicyField::Key,
                PostPolicyValue::Exact(object_key.into()),
            )
            .expect("bucket policy");

        let presigned_post = self.bucket.presign_post(policy).await.unwrap();

        let mut fields: Vec<(String, String)> = presigned_post.fields.into_iter().collect();
        fields.extend(presigned_post.dynamic_fields.into_iter());

        PostObjectUrl {
            url: presigned_post.url,
            fields,
        }
    }

    pub async fn handle_post_object_callback(
        &self,
        pub_key_header: &HeaderValue,
        auth_header: &HeaderValue,
        path: &str,
        query: &str,
        body: &str,
    ) -> Uuid {
        if verify_callback(pub_key_header, auth_header, path, query, body)
            .await
            .is_err()
        {
            panic!();
        }

        #[derive(Deserialize, Debug)]
        struct CallbackBody {
            file_id: Uuid,
            object_key: String,
            mime: String,
            size: i64,
            md5_base64: String,
            public: bool,
        }

        let body: CallbackBody =
            serde_json::from_str(body).expect("body should be valid json string");

        use base64::prelude::*;
        let md5 = BASE64_STANDARD
            .decode(body.md5_base64)
            .expect("body md5 should be valid base64");

        sqlx::query(
            "
            INSERT INTO kufang.files (id, s3_key, size, md5, mime, ref_count, public)
            VALUES ($1, $2, $3, $4, $5, 0, $6)
            ",
        )
        .bind(body.file_id)
        .bind(body.object_key)
        .bind(body.size)
        .bind(md5)
        .bind(body.mime)
        .bind(body.public)
        .execute(&self.pool)
        .await
        .unwrap();

        body.file_id
    }
}

/// https://help.aliyun.com/zh/oss/developer-reference/callback
fn get_post_object_callback(url: &str, body: Option<Vec<(&str, serde_json::Value)>>) -> String {
    let callback_body = if let Some(body) = body {
        serde_json::Map::from_iter(body.into_iter().map(|(k, v)| (k.to_string(), v)))
    } else {
        serde_json::Map::new()
    };

    let callback_body = serde_json::Value::Object(callback_body).to_string();

    let regex = Regex::new(r#""\$\{[\w\.:]+\}""#).expect("regex correct");
    let callback_body = regex.replace_all(&callback_body, |c: &Captures| {
        let mut full = c.get(0).expect("regex matched").as_str().to_string();
        full.pop();
        full.remove(0);

        full
    });

    let string = json!({
        "callbackUrl": url,
        "callbackBody": callback_body,
        "callbackBodyType": "application/json"
    })
    .to_string();

    use base64::prelude::*;
    BASE64_STANDARD.encode(string)
}

async fn verify_callback(
    pub_key_header: &HeaderValue,
    auth_header: &HeaderValue,
    path: &str,
    query: &str,
    body: &str,
) -> Result<(), ()> {
    let signed_string = format!("{path}{query}\n{body}");

    use base64::prelude::*;

    let pub_key_header = BASE64_STANDARD
        .decode(pub_key_header.as_bytes())
        .map_err(|_| ())?;

    let pub_key_url = std::str::from_utf8(&pub_key_header).map_err(|_| ())?;

    if !pub_key_url.starts_with("http://gosspublic.alicdn.com/")
        && !pub_key_url.starts_with("https://gosspublic.alicdn.com/")
    {
        return Err(());
    }

    let res = reqwest::get(pub_key_url).await.map_err(|_| ())?;
    let pub_key = res.text().await.map_err(|_| ())?;

    let pub_key = RsaPublicKey::from_public_key_pem(&pub_key).map_err(|_| ())?;
    let verify_key = pkcs1v15::VerifyingKey::<md5::Md5>::new(pub_key);

    let sign = BASE64_STANDARD
        .decode(auth_header.as_bytes())
        .map_err(|_| ())?;
    let sign: pkcs1v15::Signature = sign.as_slice().try_into().map_err(|_| ())?;

    verify_key
        .verify(signed_string.as_bytes(), &sign)
        .map_err(|_| ())
}
