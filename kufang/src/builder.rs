use std::sync::Arc;

use s3::{Bucket, Region, creds::Credentials};
use sqlx::PgPool;

use crate::Kufang;

impl Kufang {
    pub fn builder() -> KufangBuilder {
        KufangBuilder::default()
    }
}

#[derive(Debug, Default)]
pub struct KufangBuilder {
    bucket_name: Option<String>,
    bucket_region: Option<String>,
    bucket_endpoint: Option<String>,
    bucket_access_key: Option<String>,
    bucket_access_secret: Option<String>,
    db_pool: Option<PgPool>,
    s3_key_prefix: Option<String>,
}

impl KufangBuilder {
    pub fn bucket(
        mut self,
        bucket_name: impl Into<String>,
        bucket_region: impl Into<String>,
        bucket_endpoint: impl Into<String>,
    ) -> Self {
        self.bucket_name = Some(bucket_name.into());
        self.bucket_region = Some(bucket_region.into());
        self.bucket_endpoint = Some(bucket_endpoint.into());

        self
    }

    pub fn credentials(
        mut self,
        access_key: impl Into<String>,
        access_secret: impl Into<String>,
    ) -> Self {
        self.bucket_access_key = Some(access_key.into());
        self.bucket_access_secret = Some(access_secret.into());

        self
    }

    pub fn pool(mut self, pool: PgPool) -> Self {
        self.db_pool = Some(pool);

        self
    }

    pub fn key_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.s3_key_prefix = Some(prefix.into());

        self
    }

    pub fn build(self) -> Kufang {
        let bucket = Bucket::new(
            &self.bucket_name.unwrap(),
            Region::Custom {
                region: self.bucket_region.unwrap(),
                endpoint: self.bucket_endpoint.unwrap(),
            },
            Credentials {
                access_key: Some(self.bucket_access_key.unwrap()),
                secret_key: Some(self.bucket_access_secret.unwrap()),
                security_token: None,
                session_token: None,
                expiration: None,
            },
        )
        .expect("create s3 bucket");

        Kufang {
            bucket: Arc::from(bucket),
            pool: self.db_pool.unwrap(),
            s3_key_prefix: self.s3_key_prefix.unwrap().into(),
        }
    }
}
