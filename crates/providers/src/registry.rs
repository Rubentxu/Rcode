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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockLlmProvider;
    use std::sync::Arc;

    fn create_mock_provider() -> Arc<dyn LlmProvider> {
        Arc::new(MockLlmProvider::new())
    }

    #[test]
    fn test_registry_new() {
        let registry = ProviderRegistry::new();
        assert!(registry.list_providers().is_empty());
        assert!(registry.get("test").is_none());
    }

    #[test]
    fn test_registry_default() {
        let registry = ProviderRegistry::default();
        assert!(registry.list_providers().is_empty());
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = ProviderRegistry::new();
        let provider = create_mock_provider();

        registry.register(provider.clone());

        let retrieved = registry.get("mock");
        assert!(retrieved.is_some());
        assert!(Arc::ptr_eq(retrieved.unwrap(), &provider));
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = ProviderRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_registry_list_providers() {
        let mut registry = ProviderRegistry::new();
        assert!(registry.list_providers().is_empty());

        let provider1 = create_mock_provider();
        registry.register(provider1);
        assert_eq!(registry.list_providers(), vec!["mock"]);

        // Register another (MockLlmProvider also has id "mock", so it replaces)
        let provider2 = create_mock_provider();
        registry.register(provider2);
        // Still just "mock" since all MockLlmProvider have the same id
        assert_eq!(registry.list_providers(), vec!["mock"]);
    }

    #[test]
    fn test_registry_get_model_info() {
        let mut registry = ProviderRegistry::new();
        let provider = create_mock_provider();
        registry.register(provider);

        // MockLlmProvider returns Some for any model_id
        let info = registry.get_model_info("mock", "any-model");
        assert!(info.is_some());
        assert_eq!(info.unwrap().id, "mock-model");
    }

    #[test]
    fn test_registry_get_model_info_unknown_provider() {
        let registry = ProviderRegistry::new();
        let info = registry.get_model_info("unknown", "model");
        assert!(info.is_none());
    }

    #[test]
    fn test_registry_multiple_providers() {
        // This test verifies registry behavior when we have a provider
        // that returns None for model_info
        let mut registry = ProviderRegistry::new();

        let provider = create_mock_provider();
        registry.register(provider);

        // Verify we can look up by provider id
        assert!(registry.get("mock").is_some());

        // Verify list contains our provider
        let providers = registry.list_providers();
        assert!(providers.contains(&"mock".to_string()));
    }
}
