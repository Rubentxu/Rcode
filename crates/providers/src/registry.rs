//! Provider registry

use std::collections::HashMap;
use std::sync::Arc;

use super::LlmProvider;
use rcode_core::ModelInfo;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn LlmProvider>) {
        self.providers
            .insert(provider.provider_id().to_string(), provider);
    }

    pub fn get(&self, provider_id: &str) -> Option<&Arc<dyn LlmProvider>> {
        self.providers.get(provider_id)
    }

    pub fn list_providers(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    pub fn get_model_info(&self, provider_id: &str, model_id: &str) -> Option<ModelInfo> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.model_info(model_id))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
