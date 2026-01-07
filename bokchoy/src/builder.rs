use std::sync::Arc;

use sqlx::PgConnection;

use crate::{PaymentService, Provider, psp::PaymentServiceProvider, repo::PaymentRepo};

#[derive(Default)]
pub struct PaymentServiceBuilder {
    providers: Vec<(Provider, Box<dyn PaymentServiceProvider + Send + Sync>)>,
}

impl PaymentServiceBuilder {
    pub fn register(&mut self, key: Provider, provider: impl PaymentServiceProvider + 'static) {
        self.providers.push((key, Box::new(provider)));
    }

    pub fn build(self) -> PaymentService<PgConnection> {
        let providers = self.providers.into_iter().collect();

        PaymentService {
            providers: Arc::new(providers),
            repo: Arc::new(PaymentRepo),
        }
    }
}
