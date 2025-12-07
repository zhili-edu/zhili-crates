use std::sync::Arc;

use crate::{PaymentService, Provider, psp::PaymentServiceProvider};

#[derive(Default)]
pub struct PaymentServiceBuilder {
    providers: Vec<(Provider, Box<dyn PaymentServiceProvider + Send + Sync>)>,
}

impl PaymentServiceBuilder {
    pub fn register(&mut self, key: Provider, provider: impl PaymentServiceProvider + 'static) {
        self.providers.push((key, Box::new(provider)));
    }

    pub fn build(self) -> PaymentService {
        let providers = self.providers.into_iter().collect();

        PaymentService {
            providers: Arc::new(providers),
        }
    }
}
