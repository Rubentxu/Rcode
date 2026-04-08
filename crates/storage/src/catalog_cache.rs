//! Model catalog cache repository for SQLite persistence

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::StorageError;

/// A model entry in the catalog cache
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedModel {
    pub id: String,
    pub provider: String,
    pub display_name: String,
    pub has_credentials: bool,
    pub source: String, // "api", "fallback", "configured"
    pub enabled: bool,
}

/// Cached entry from the model catalog
#[derive(Debug, Clone)]
pub struct CachedCatalogEntry {
    pub provider_id: String,
    pub models: Vec<CachedModel>,
    pub updated_at: SystemTime,
}

impl CachedCatalogEntry {
    /// Check if this cache entry is still valid (within TTL)
    pub fn is_valid(&self, ttl: Duration) -> bool {
        self.updated_at
            .elapsed()
            .map(|elapsed| elapsed < ttl)
            .unwrap_or(false)
    }
}

pub struct CatalogCacheRepository {
    conn: Mutex<Connection>,
}

impl CatalogCacheRepository {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn: Mutex::new(conn),
        }
    }

    /// Load all cached catalog entries
    pub fn get_all(&self) -> Result<Vec<CachedCatalogEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT provider_id, models, updated_at FROM model_catalog_cache")
            .context("Failed to prepare statement")?;

        let rows = stmt
            .query_map([], |row| {
                let provider_id: String = row.get(0)?;
                let models_json: String = row.get(1)?;
                let updated_at_str: String = row.get(2)?;

                // Parse updated_at from RFC3339 format
                let updated_at = chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map(|dt| SystemTime::UNIX_EPOCH + Duration::from_secs(dt.timestamp() as u64))
                    .unwrap_or_else(SystemTime::now);

                Ok((provider_id, models_json, updated_at))
            })
            .context("Failed to query catalog cache")?;

        let mut entries = Vec::new();
        for row_result in rows {
            let (provider_id, models_json, updated_at) =
                row_result.context("Failed to read catalog cache row")?;

            // Parse models JSON
            let models: Vec<CachedModel> =
                serde_json::from_str(&models_json).unwrap_or_else(|_| Vec::new());

            entries.push(CachedCatalogEntry {
                provider_id,
                models,
                updated_at,
            });
        }

        Ok(entries)
    }

    /// Upsert a batch of catalog entries
    pub fn upsert_batch(&self, entries: &[CachedCatalogEntry]) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        for entry in entries {
            let models_json = serde_json::to_string(&entry.models)?;
            let updated_at_str = chrono::Utc::now().to_rfc3339();

            conn.execute(
                r#"
                INSERT INTO model_catalog_cache (provider_id, models, updated_at)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(provider_id) DO UPDATE SET
                    models = excluded.models,
                    updated_at = excluded.updated_at
                "#,
                params![entry.provider_id, models_json, updated_at_str],
            )
            .context("Failed to upsert catalog cache entry")?;
        }

        Ok(())
    }

    /// Clear stale cache entries (older than TTL)
    pub fn clear_stale(&self, ttl: Duration) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StorageError::LockPoisoned(e.to_string()))?;

        let cutoff = SystemTime::now()
            .checked_sub(ttl)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let cutoff_str = chrono::DateTime::<chrono::Utc>::from(cutoff).to_rfc3339();

        let deleted = conn
            .execute(
                "DELETE FROM model_catalog_cache WHERE updated_at < ?1",
                params![cutoff_str],
            )
            .context("Failed to clear stale catalog cache")?;

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_catalog_entry_is_valid() {
        let entry = CachedCatalogEntry {
            provider_id: "test".into(),
            models: vec![],
            updated_at: SystemTime::now(),
        };

        assert!(entry.is_valid(Duration::from_secs(3600)));
        assert!(!entry.is_valid(Duration::from_secs(0)));
    }

    #[test]
    fn test_catalog_cache_roundtrip() {
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

        // Insert an entry
        let entry = CachedCatalogEntry {
            provider_id: "anthropic".into(),
            models: vec![CachedModel {
                id: "anthropic/claude-1".into(),
                provider: "anthropic".into(),
                display_name: "Claude 1".into(),
                has_credentials: true,
                source: "api".into(),
                enabled: true,
            }],
            updated_at: SystemTime::now(),
        };

        repo.upsert_batch(std::slice::from_ref(&entry)).unwrap();

        // Load it back
        let loaded = repo.get_all().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].provider_id, "anthropic");
        assert_eq!(loaded[0].models.len(), 1);
    }
}
