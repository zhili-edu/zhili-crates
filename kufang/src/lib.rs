use md5::{Digest, Md5};
use sqlx::{PgPool, query};
use std::sync::Arc;
use uuid::Uuid;

use s3::Bucket;

mod builder;
pub mod migration;
mod post;

pub use post::PostObjectUrl;

#[derive(Clone)]
pub struct Kufang {
    bucket: Arc<Bucket>,
    pool: PgPool,
    s3_key_prefix: Arc<str>,
}

impl Kufang {
    fn get_s3_key(&self, file_id: Uuid, public: bool) -> String {
        let time = {
            let time = file_id
                .get_timestamp()
                .expect("extract timestamp from uuid v7");

            let (secs, nanos) = time.to_unix();

            time::UtcDateTime::from_unix_timestamp(
                secs.try_into().expect("timestamp secs in range of i64"),
            )
            .expect("valid timestamp")
                + time::Duration::nanoseconds(nanos.into())
        };

        format!(
            "{}/{}/{}/{}/{}/{}",
            self.s3_key_prefix,
            if public { "public" } else { "private" },
            time.year(),
            time.month() as u8,
            time.day(),
            file_id,
        )
    }

    pub async fn upload_file(&self, file: &[u8], mime: &str, public: bool) {
        let id = Uuid::now_v7();

        self.upload_file_with_id(id, file, mime, public).await;
    }

    pub async fn upload_file_with_id(&self, id: Uuid, file: &[u8], mime: &str, public: bool) {
        let key = self.get_s3_key(id, public);

        let md5 = Md5::digest(file).to_vec();

        self.bucket
            .put_object_with_content_type(&key, file, mime)
            .await
            .unwrap();

        query(
            "
            INSERT INTO kufang.files (id, s3_key, size, md5, mime, ref_count, public)
            VALUES ($1, $2, $3, $4, $5, 0, $6)
            ",
        )
        .bind(id)
        .bind(key)
        .bind(file.len() as i64)
        .bind(md5)
        .bind(mime)
        .bind(public)
        .execute(&self.pool)
        .await
        .unwrap();
    }
}
