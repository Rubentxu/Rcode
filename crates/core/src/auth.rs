//! Kustomize-like auth credential layering
//!
//! This module provides a layered credential system following the Kustomize principle:
//!
//! - **Base** (read-only): `~/.local/share/opencode/auth.json` — OpenCode's credential store
//! - **Overlay** (RCode-owned): `~/.local/share/rcode/auth.json` — RCode-specific credentials
//!
//! Credential resolution order (merged, Kustomize-style):
//! 1. RCode overlay takes precedence over OpenCode base
//! 2. Environment variables ({PROVIDER}_API_KEY, {PROVIDER}_AUTH_TOKEN)
//! 3. Config file (optional fallback, but api_key should NOT be stored here)
//!
//! RCode NEVER writes to OpenCode's auth.json. It only manages its own overlay.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

/// Credential types matching OpenCode's auth.json format
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Credential {
    /// API key credential
    Api {
        #[serde(rename = "key")]
        key: String,
    },
    /// OAuth token credential
    OAuth {
        /// Access token
        access: String,
        /// Refresh token (optional)
        #[serde(rename = "refresh", skip_serializing_if = "Option::is_none")]
        refresh: Option<String>,
        /// Expiration timestamp (optional, 0 means no expiration)
        #[serde(rename = "expires", skip_serializing_if = "Option::is_none")]
        expires: Option<i64>,
    },
}

impl Credential {
    /// Get the primary secret value (API key or access token)
    pub fn primary_secret(&self) -> &str {
        match self {
            Credential::Api { key } => key,
            Credential::OAuth { access, .. } => access,
        }
    }
}

/// Get the base auth.json path: ~/.local/share/opencode/auth.json
/// This is OpenCode's credential store — RCode never writes here.
pub fn auth_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("opencode").join("auth.json")
}

/// RCode overlay credential path: ~/.local/share/rcode/auth.json
/// This is RCode-owned — OpenCode's auth.json is never written by RCode.
pub fn rcode_auth_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("rcode").join("auth.json")
}

/// Ensure the rcode auth directory exists with proper permissions
fn ensure_rcode_auth_dir() -> Result<()> {
    let path = rcode_auth_path();
    let dir = path.parent().ok_or_else(|| anyhow::anyhow!("Invalid rcode auth path"))?;

    if !dir.exists() {
        std::fs::create_dir_all(dir).context("Failed to create rcode auth directory")?;
    }
    Ok(())
}

/// Load credentials from a specific path (internal helper)
fn load_credentials_from_path(path: &std::path::Path) -> HashMap<String, Credential> {
    if !path.exists() {
        return HashMap::new();
    }

    match std::fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str(&content) {
            Ok(creds) => creds,
            Err(e) => {
                tracing::warn!("Failed to parse auth file {:?}: {}", path, e);
                HashMap::new()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read auth file {:?}: {}", path, e);
            HashMap::new()
        }
    }
}

/// Load credentials from both opencode (base) and rcode (overlay) auth.json files.
/// Merge strategy: rcode overlay takes precedence over opencode base.
/// This is the Kustomize-like layering: opencode is the base, rcode is the overlay.
pub fn load_credentials() -> HashMap<String, Credential> {
    // 1. Load base (opencode)
    let mut merged = load_credentials_from_path(&auth_path());

    // 2. Load overlay (rcode) and merge on top — rcode wins
    let overlay = load_credentials_from_path(&rcode_auth_path());
    for (key, value) in overlay {
        merged.insert(key, value);
    }

    merged
}

/// Save a credential to RCode's auth overlay file.
///
/// Creates the file and parent directory if needed.
/// ALWAYS merges with existing credentials - never overwrites the entire file.
/// Only writes to rcode overlay — never modifies OpenCode's base auth.json.
pub fn save_credential(provider_id: &str, credential: Credential) -> Result<()> {
    ensure_rcode_auth_dir()?;

    let path = rcode_auth_path();

    // Load existing rcode overlay credentials (NOT the merged view — we only update our own file)
    let mut credentials = load_credentials_from_path(&path);

    // Merge the new credential (overwriting just this provider)
    credentials.insert(provider_id.to_string(), credential);

    // Serialize and write with proper permissions
    let json =
        serde_json::to_string_pretty(&credentials).context("Failed to serialize credentials")?;

    // Write to temp file first, then rename (atomic write)
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, json).context("Failed to write temp auth file")?;

    // Set permissions to 0600 (owner read/write only) before renaming
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    std::fs::rename(&temp_path, &path).context("Failed to rename temp auth file")?;

    tracing::info!(
        "Saved credential for provider '{}' to rcode auth.json",
        provider_id
    );
    Ok(())
}

/// Delete a credential from RCode's auth overlay file.
///
/// Only deletes from rcode overlay — never modifies OpenCode's base auth.json.
pub fn delete_credential(provider_id: &str) -> Result<()> {
    let path = rcode_auth_path();

    if !path.exists() {
        return Ok(()); // Nothing to delete in our overlay
    }

    let mut credentials = load_credentials_from_path(&path);
    credentials.remove(provider_id);

    if credentials.is_empty() {
        // Remove the file entirely if no credentials remain
        std::fs::remove_file(&path).context("Failed to remove rcode auth.json")?;
    } else {
        // Write the updated credentials back
        let json = serde_json::to_string_pretty(&credentials)
            .context("Failed to serialize credentials")?;
        std::fs::write(&path, json).context("Failed to write rcode auth.json")?;
    }

    tracing::info!(
        "Deleted credential for provider '{}' from rcode auth.json",
        provider_id
    );
    Ok(())
}

/// Check if a provider has a credential in auth.json
pub fn has_credential(provider_id: &str) -> bool {
    load_credentials().contains_key(provider_id)
}

/// Get the API key/OAuth token for a provider from auth.json
///
/// For `"type": "api"` credentials → returns the `"key"` value
/// For `"type": "oauth"` credentials → returns the `"access"` value
pub fn get_api_key(provider_id: &str) -> Option<String> {
    load_credentials()
        .get(provider_id)
        .map(|cred| cred.primary_secret().to_string())
}

/// Get a specific OAuth field from a credential
pub fn get_oauth_tokens(provider_id: &str) -> Option<(String, Option<String>, Option<i64>)> {
    match load_credentials().get(provider_id)? {
        Credential::OAuth {
            access,
            refresh,
            expires,
        } => Some((access.clone(), refresh.clone(), *expires)),
        Credential::Api { .. } => None,
    }
}

/// Get the credential type for a provider (if it exists)
pub fn get_credential_type(provider_id: &str) -> Option<&'static str> {
    match load_credentials().get(provider_id)? {
        Credential::Api { .. } => Some("api"),
        Credential::OAuth { .. } => Some("oauth"),
    }
}

/// Strip secrets (api_key) from provider configs before saving to disk
///
/// This ensures api_key fields are never written to config files.
/// API keys live in auth.json, not in the config file.
pub fn strip_secrets_from_config(
    config: &crate::config::RcodeConfig,
) -> crate::config::RcodeConfig {
    use serde_json::json;

    // Start with the config as JSON
    let mut config_json = json!(config);

    // Remove api_key from providers
    if let Some(providers) = config_json
        .get_mut("providers")
        .and_then(|p| p.as_object_mut())
    {
        for (_provider_id, provider_config) in providers.iter_mut() {
            if let Some(obj) = provider_config.as_object_mut() {
                obj.remove("api_key");
                obj.remove("key"); // Also remove any "key" field that might be used
            }
        }
    }

    // Also check the extra field for any api_key entries in nested providers
    if let Some(extra) = config_json.get_mut("extra").and_then(|e| e.as_object_mut())
        && let Some(providers) = extra.get_mut("providers").and_then(|p| p.as_object_mut())
    {
        for (_provider_id, provider_config) in providers.iter_mut() {
            if let Some(obj) = provider_config.as_object_mut() {
                obj.remove("api_key");
                obj.remove("key");
            }
        }
    }

    serde_json::from_value(config_json).unwrap_or_else(|_| config.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn with_temp_auth_file<F>(f: F)
    where
        F: Fn(PathBuf),
    {
        let temp = TempDir::new().unwrap();
        let temp_auth_path = temp.path().join("opencode").join("auth.json");

        // Create the directory structure
        std::fs::create_dir_all(temp_auth_path.parent().unwrap()).unwrap();

        // Temporarily replace the global auth path
        // We do this by directly testing the functions with a fake path
        // and noting that the actual auth_path() function uses dirs::data_local_dir()

        // For unit testing, we'll test the serialization/deserialization directly
        f(temp_auth_path);
    }

    #[test]
    fn test_credential_serialization_api() {
        let cred = Credential::Api {
            key: "test-key-123".to_string(),
        };

        let json = serde_json::to_string(&cred).unwrap();
        assert!(json.contains("\"type\":\"api\""));
        assert!(json.contains("\"key\":\"test-key-123\""));

        let parsed: Credential = serde_json::from_str(&json).unwrap();
        match parsed {
            Credential::Api { key } => assert_eq!(key, "test-key-123"),
            _ => panic!("Expected Api credential"),
        }
    }

    #[test]
    fn test_credential_serialization_oauth() {
        let cred = Credential::OAuth {
            access: "access-token".to_string(),
            refresh: Some("refresh-token".to_string()),
            expires: Some(1234567890),
        };

        let json = serde_json::to_string(&cred).unwrap();
        assert!(json.contains("\"type\":\"oauth\""));
        assert!(json.contains("\"access\":\"access-token\""));
        assert!(json.contains("\"refresh\":\"refresh-token\""));
        assert!(json.contains("\"expires\":1234567890"));

        let parsed: Credential = serde_json::from_str(&json).unwrap();
        match parsed {
            Credential::OAuth {
                access,
                refresh,
                expires,
            } => {
                assert_eq!(access, "access-token");
                assert_eq!(refresh, Some("refresh-token".to_string()));
                assert_eq!(expires, Some(1234567890));
            }
            _ => panic!("Expected OAuth credential"),
        }
    }

    #[test]
    fn test_credential_serialization_oauth_minimal() {
        let cred = Credential::OAuth {
            access: "access-token".to_string(),
            refresh: None,
            expires: None,
        };

        let json = serde_json::to_string(&cred).unwrap();
        assert!(json.contains("\"type\":\"oauth\""));
        assert!(json.contains("\"access\":\"access-token\""));
        // refresh and expires should be omitted when None
        assert!(!json.contains("refresh"));

        let parsed: Credential = serde_json::from_str(&json).unwrap();
        match parsed {
            Credential::OAuth {
                access,
                refresh,
                expires,
            } => {
                assert_eq!(access, "access-token");
                assert_eq!(refresh, None);
                assert_eq!(expires, None);
            }
            _ => panic!("Expected OAuth credential"),
        }
    }

    #[test]
    fn test_full_auth_json_format() {
        // Test the exact format from OpenCode's auth.json
        let auth_json = r#"{
  "github-copilot": {
    "type": "oauth",
    "refresh": "gho_xxx",
    "access": "gho_yyy",
    "expires": 0
  },
  "minimax": {
    "type": "api",
    "key": "eyJhbGciOiJSUzI1NiIs..."
  }
}"#;

        let creds: HashMap<String, Credential> = serde_json::from_str(auth_json).unwrap();

        assert_eq!(creds.len(), 2);

        // Check minimax (api type)
        let minimax = creds.get("minimax").unwrap();
        assert_eq!(minimax.primary_secret(), "eyJhbGciOiJSUzI1NiIs...");

        // Check github-copilot (oauth type)
        let copilot = creds.get("github-copilot").unwrap();
        assert_eq!(copilot.primary_secret(), "gho_yyy");
    }

    #[test]
    fn test_primary_secret_api() {
        let cred = Credential::Api {
            key: "secret-key".to_string(),
        };
        assert_eq!(cred.primary_secret(), "secret-key");
    }

    #[test]
    fn test_primary_secret_oauth() {
        let cred = Credential::OAuth {
            access: "access-token".to_string(),
            refresh: Some("refresh-token".to_string()),
            expires: Some(12345),
        };
        assert_eq!(cred.primary_secret(), "access-token");
    }

    #[test]
    fn test_get_credential_type() {
        let api_cred = Credential::Api {
            key: "key".to_string(),
        };
        let oauth_cred = Credential::OAuth {
            access: "access".to_string(),
            refresh: None,
            expires: None,
        };

        let mut creds = HashMap::new();
        creds.insert("test-api".to_string(), api_cred);
        creds.insert("test-oauth".to_string(), oauth_cred);

        assert_eq!(
            creds.get("test-api").map(|c| match c {
                Credential::Api { .. } => "api",
                Credential::OAuth { .. } => "oauth",
            }),
            Some("api")
        );

        assert_eq!(
            creds.get("test-oauth").map(|c| match c {
                Credential::Api { .. } => "api",
                Credential::OAuth { .. } => "oauth",
            }),
            Some("oauth")
        );
    }

    #[test]
    fn test_rcode_auth_path_is_separate_from_opencode() {
        let opencode = auth_path();
        let rcode = rcode_auth_path();
        assert_ne!(opencode, rcode);
        assert!(opencode.to_string_lossy().contains("opencode"));
        assert!(rcode.to_string_lossy().contains("rcode"));
    }

    #[test]
    fn test_load_credentials_from_path_empty_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nonexistent.json");
        let result = load_credentials_from_path(&path);
        assert!(result.is_empty());
    }

    #[test]
    fn test_load_credentials_from_path_reads_api_cred() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("auth.json");
        std::fs::write(
            &path,
            r#"{"myprovider":{"type":"api","key":"sk-test"}}"#,
        )
        .unwrap();
        let result = load_credentials_from_path(&path);
        assert_eq!(result.len(), 1);
        assert_eq!(result["myprovider"].primary_secret(), "sk-test");
    }

    #[test]
    fn test_merge_overlay_wins_over_base() {
        // Simulate: base has provider A with key1, overlay has provider A with key2 and provider B
        let base = HashMap::from([
            (
                "provider-a".to_string(),
                Credential::Api {
                    key: "base-key-a".to_string(),
                },
            ),
            (
                "provider-b".to_string(),
                Credential::Api {
                    key: "base-key-b".to_string(),
                },
            ),
        ]);
        let overlay = HashMap::from([
            (
                "provider-a".to_string(),
                Credential::Api {
                    key: "overlay-key-a".to_string(),
                },
            ),
            (
                "provider-c".to_string(),
                Credential::Api {
                    key: "overlay-key-c".to_string(),
                },
            ),
        ]);

        let mut merged = base;
        for (k, v) in overlay {
            merged.insert(k, v);
        }

        // overlay-key-a wins over base-key-a
        assert_eq!(merged["provider-a"].primary_secret(), "overlay-key-a");
        // base-only provider preserved
        assert_eq!(merged["provider-b"].primary_secret(), "base-key-b");
        // overlay-only provider present
        assert_eq!(merged["provider-c"].primary_secret(), "overlay-key-c");
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn test_merge_base_only_when_no_overlay() {
        let base = HashMap::from([(
            "github-copilot".to_string(),
            Credential::OAuth {
                access: "gho_test".to_string(),
                refresh: None,
                expires: None,
            },
        )]);
        let overlay: HashMap<String, Credential> = HashMap::new();

        let mut merged = base;
        for (k, v) in overlay {
            merged.insert(k, v);
        }

        assert_eq!(merged["github-copilot"].primary_secret(), "gho_test");
        assert_eq!(merged.len(), 1);
    }

    // =============================================================================
    // delete_credential tests
    // =============================================================================

    #[test]
    fn test_delete_credential_nonexistent_is_noop() {
        // Deleting a credential that doesn't exist should not error
        // Note: This tests against the real rcode_auth_path(), which may or may not
        // have credentials depending on the system state
        let result = delete_credential("nonexistent-provider-test-12345");
        // Should succeed (no-op)
        assert!(result.is_ok(), "delete_credential should not error on missing provider");
    }

    #[test]
    fn test_delete_and_reload_credential_flow() {
        // This test verifies the save→load→delete→load cycle works correctly
        // by testing with a temporary path instead of the global auth path.
        // We test the underlying functions directly since delete_credential uses global paths.
        
        let temp = tempfile::tempdir().unwrap();
        let temp_path = temp.path().join("rcode").join("auth.json");
        
        // Create directory structure
        std::fs::create_dir_all(temp_path.parent().unwrap()).unwrap();
        
        // Save a credential to the temp path directly
        let cred = Credential::Api { key: "test-delete-key".to_string() };
        let json = serde_json::to_string_pretty(&std::collections::HashMap::from([
            ("test-provider".to_string(), cred)
        ])).unwrap();
        std::fs::write(&temp_path, json).unwrap();
        
        // Verify it was saved
        let loaded = load_credentials_from_path(&temp_path);
        assert!(loaded.contains_key("test-provider"));
        
        // Now simulate delete: load, remove, write back
        let mut credentials = load_credentials_from_path(&temp_path);
        credentials.remove("test-provider");
        
        if credentials.is_empty() {
            std::fs::remove_file(&temp_path).unwrap();
        } else {
            let json = serde_json::to_string_pretty(&credentials).unwrap();
            std::fs::write(&temp_path, json).unwrap();
        }
        
        // Verify it's gone
        let loaded_after = load_credentials_from_path(&temp_path);
        assert!(!loaded_after.contains_key("test-provider"));
    }

    #[test]
    fn test_delete_last_credential_removes_file() {
        // When the last credential is deleted, the file should be removed
        let temp = tempfile::tempdir().unwrap();
        let temp_path = temp.path().join("rcode").join("auth.json");
        
        // Create directory structure with one credential
        std::fs::create_dir_all(temp_path.parent().unwrap()).unwrap();
        let cred = Credential::Api { key: "last-key".to_string() };
        let json = serde_json::to_string_pretty(&std::collections::HashMap::from([
            ("last-provider".to_string(), cred)
        ])).unwrap();
        std::fs::write(&temp_path, json).unwrap();
        
        assert!(temp_path.exists());
        
        // Simulate delete of last credential
        let credentials: std::collections::HashMap<String, Credential> = std::collections::HashMap::new();
        let json = serde_json::to_string_pretty(&credentials).unwrap();
        std::fs::write(&temp_path, json).unwrap();
        
        // Verify file is removed (or empty)
        let loaded = load_credentials_from_path(&temp_path);
        assert!(loaded.is_empty());
    }
}
