use md5::{Digest, Md5};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

use s3::Bucket;

mod builder;
pub mod migration;
mod post;

pub use post::PostObjectUrl;

#[derive(Clone)]
pub struct Kufang {
    bucket: Arc<Bucket>,
    pool: sqlx::PgPool,
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

    pub async fn upload_file(&self, file: &[u8], mime: &str, public: bool) -> Uuid {
        let id = Uuid::now_v7();

        self.upload_file_with_id(id, file, mime, public).await;

        id
    }

    pub async fn upload_file_with_id(&self, id: Uuid, file: &[u8], mime: &str, public: bool) {
        let key = self.get_s3_key(id, public);

        let md5 = Md5::digest(file).to_vec();

        self.bucket
            .put_object_with_content_type(&key, file, mime)
            .await
            .unwrap();

        sqlx::query(
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

    pub async fn get_file_s3_key(&self, id: Uuid) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>("SELECT s3_key FROM kufang.files WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn get_s3_key_map(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, String>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, s3_key FROM kufang.files WHERE id = ANY($1::uuid[])",
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await
        .map(|r| r.into_iter().collect::<HashMap<_, _>>())
    }

    pub async fn get_file_id_by_md5(&self, md5: &[u8]) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM kufang.files WHERE md5 = $1")
            .bind(md5)
            .fetch_optional(&self.pool)
            .await
    }
}
