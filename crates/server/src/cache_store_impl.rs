//! Cache store implementation for rcode-storage integration
//!
//! Implements the `CacheStore` trait from rcode-providers using
//! rcode-storage's CatalogCacheRepository.

use std::collections::HashMap;
use std::sync::Arc;

use rcode_providers::catalog::{CacheStore, CatalogModel, ModelSource};
use rcode_storage::catalog_cache::{CachedCatalogEntry, CachedModel, CatalogCacheRepository};

/// Adapter that implements `CacheStore` using `CatalogCacheRepository`.
pub struct ServerCacheStore {
    repo: Arc<CatalogCacheRepository>,
}

impl ServerCacheStore {
    pub fn new(repo: CatalogCacheRepository) -> Self {
        Self {
            repo: Arc::new(repo),
        }
    }

    /// Convert a CachedModel to CatalogModel
    fn cached_model_to_catalog(cached: CachedModel) -> CatalogModel {
        let source = match cached.source.as_str() {
            "api" => ModelSource::Api,
            "fallback" => ModelSource::Fallback,
            "configured" => ModelSource::Configured,
            _ => ModelSource::Fallback,
        };
        CatalogModel {
            id: cached.id,
            provider: cached.provider,
            display_name: cached.display_name,
            has_credentials: cached.has_credentials,
            source,
            enabled: cached.enabled,
        }
    }

    /// Convert a CatalogModel to CachedModel
    fn catalog_model_to_cached(catalog: &CatalogModel) -> CachedModel {
        let source = match catalog.source {
            ModelSource::Api => "api".to_string(),
            ModelSource::Fallback => "fallback".to_string(),
            ModelSource::Configured => "configured".to_string(),
        };
        CachedModel {
            id: catalog.id.clone(),
            provider: catalog.provider.clone(),
            display_name: catalog.display_name.clone(),
            has_credentials: catalog.has_credentials,
            source,
            enabled: catalog.enabled,
        }
    }
}

impl CacheStore for ServerCacheStore {
    fn get_all_cached(&self) -> HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)> {
        let entries = match self.repo.get_all() {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to load catalog cache: {}", e);
                return HashMap::new();
            }
        };

        let mut result: HashMap<String, (Vec<CatalogModel>, std::time::SystemTime)> =
            HashMap::new();
        for entry in entries {
            let models: Vec<CatalogModel> = entry
                .models
                .into_iter()
                .map(Self::cached_model_to_catalog)
                .collect();
            result.insert(entry.provider_id, (models, entry.updated_at));
        }
        result
    }

    fn save_cached_models(&self, provider_id: &str, models: &[CatalogModel]) {
        let cached_models: Vec<CachedModel> =
            models.iter().map(Self::catalog_model_to_cached).collect();

        let entry = CachedCatalogEntry {
            provider_id: provider_id.to_string(),
            models: cached_models,
            updated_at: std::time::SystemTime::now(),
        };

        if let Err(e) = self.repo.upsert_batch(&[entry]) {
            tracing::warn!("Failed to persist catalog cache for {}: {}", provider_id, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcode_storage::catalog_cache::CatalogCacheRepository;
    use rusqlite::Connection;

    #[test]
    fn test_cache_store_roundtrip() {
        // Create an in-memory repository
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE model_catalog_cache (
                provider_id TEXT PRIMARY KEY,
                models TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        let repo = CatalogCacheRepository::new(conn);
        let store = ServerCacheStore::new(repo);

        // Create test models
        let models = vec![
            CatalogModel {
                id: "anthropic/claude-1".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude 1".to_string(),
                has_credentials: true,
                source: ModelSource::Api,
                enabled: true,
            },
            CatalogModel {
                id: "anthropic/claude-2".to_string(),
                provider: "anthropic".to_string(),
                display_name: "Claude 2".to_string(),
                has_credentials: true,
                source: ModelSource::Api,
                enabled: true,
            },
        ];

        // Save to cache
        store.save_cached_models("anthropic", &models);

        // Load from cache
        let loaded = store.get_all_cached();
        assert!(loaded.contains_key("anthropic"));
        let (loaded_models, _updated_at) = loaded.get("anthropic").unwrap();
        assert_eq!(loaded_models.len(), 2);
        assert_eq!(loaded_models[0].id, "anthropic/claude-1");
        assert_eq!(loaded_models[1].id, "anthropic/claude-2");
    }

    #[test]
    fn test_cache_store_converts_source_correctly() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE model_catalog_cache (
                provider_id TEXT PRIMARY KEY,
                models TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        let repo = CatalogCacheRepository::new(conn);
        let store = ServerCacheStore::new(repo);

        let models = vec![
            CatalogModel {
                id: "test/model1".to_string(),
                provider: "test".to_string(),
                display_name: "Model 1".to_string(),
                has_credentials: true,
                source: ModelSource::Api,
                enabled: true,
            },
            CatalogModel {
                id: "test/model2".to_string(),
                provider: "test".to_string(),
                display_name: "Model 2".to_string(),
                has_credentials: false,
                source: ModelSource::Fallback,
                enabled: false,
            },
        ];

        store.save_cached_models("test", &models);
        let loaded = store.get_all_cached();
        let (loaded_models, _updated_at) = loaded.get("test").unwrap();

        assert_eq!(loaded_models[0].source, ModelSource::Api);
        assert_eq!(loaded_models[1].source, ModelSource::Fallback);
    }

    #[test]
    fn test_cache_store_handles_empty_cache() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE model_catalog_cache (
                provider_id TEXT PRIMARY KEY,
                models TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        let repo = CatalogCacheRepository::new(conn);
        let store = ServerCacheStore::new(repo);

        let loaded = store.get_all_cached();
        assert!(loaded.is_empty());
    }
}
