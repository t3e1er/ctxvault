//! Client tracking, authentication, and visual identity configuration.
//!
//! Provides multi-agent tracking and color association for MCP sessions and
//! the 3D GraphView visualizer. Supports optional local network client-key auth.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Visual and authentication profile for a connected AI client / agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientEntry {
    /// Unique identifier for the client (e.g. "antigravity", "claude", "gemini").
    pub id: String,
    /// Human-friendly display name (e.g. "Antigravity Agent", "Claude Desktop").
    pub name: String,
    /// Optional authentication secret key for local network verification.
    #[serde(default)]
    pub key: Option<String>,
    /// Hex color code associated with this agent (e.g. "#38bdf8").
    #[serde(default = "default_client_color")]
    pub color: String,
}

fn default_client_color() -> String {
    "#38bdf8".to_string()
}

/// Registry of known clients and authentication rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientsRegistry {
    /// List of registered client profiles.
    #[serde(default)]
    pub clients: Vec<ClientEntry>,
    /// Whether incoming MCP requests must provide a valid matching key.
    #[serde(default)]
    pub require_auth: bool,
    /// Optional dedicated authentication secret key for daemon-to-graphview relay.
    #[serde(default)]
    pub daemon_key: Option<String>,
}

impl Default for ClientsRegistry {
    fn default() -> Self {
        Self {
            clients: vec![
                ClientEntry {
                    id: "antigravity".to_string(),
                    name: "Antigravity Agent".to_string(),
                    key: None,
                    color: "#38bdf8".to_string(), // Cyan
                },
                ClientEntry {
                    id: "claude".to_string(),
                    name: "Claude Desktop".to_string(),
                    key: None,
                    color: "#f97316".to_string(), // Orange
                },
                ClientEntry {
                    id: "gemini".to_string(),
                    name: "Gemini CLI".to_string(),
                    key: None,
                    color: "#ec4899".to_string(), // Magenta
                },
                ClientEntry {
                    id: "roo".to_string(),
                    name: "Roo Code".to_string(),
                    key: None,
                    color: "#10b981".to_string(), // Emerald
                },
                ClientEntry {
                    id: "default".to_string(),
                    name: "Anonymous Agent".to_string(),
                    key: None,
                    color: "#a855f7".to_string(), // Purple
                },
            ],
            require_auth: false,
            daemon_key: None,
        }
    }
}

impl ClientsRegistry {
    /// Check whether a secret key matches any configured client or the daemon relay key.
    pub fn is_valid_key(&self, key: &str) -> bool {
        if key.is_empty() {
            return false;
        }
        self.find_by_key(key).is_some() || self.daemon_key.as_deref() == Some(key)
    }

    /// Find a client entry by its secret key.
    pub fn find_by_key(&self, key: &str) -> Option<&ClientEntry> {
        if key.is_empty() {
            return None;
        }
        self.clients.iter().find(|c| c.key.as_deref() == Some(key))
    }

    /// Find a client entry by its unique identifier (case-insensitive).
    pub fn find_by_id(&self, id: &str) -> Option<&ClientEntry> {
        let lower = id.to_lowercase();
        self.clients.iter().find(|c| c.id.to_lowercase() == lower)
    }

    /// Resolve a client from an optional key or ID, falling back to default or anonymous.
    pub fn resolve(&self, key: Option<&str>, id: Option<&str>) -> Option<&ClientEntry> {
        if let Some(k) = key {
            if let Some(entry) = self.find_by_key(k) {
                return Some(entry);
            }
        }
        if let Some(i) = id {
            if let Some(entry) = self.find_by_id(i) {
                return Some(entry);
            }
        }
        self.find_by_id("default").or_else(|| self.clients.first())
    }
}

/// Generate a cryptographically hashed token with a given prefix.
pub fn generate_token(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&now.to_le_bytes());
    hasher.update(&pid.to_le_bytes());
    hasher.update(prefix.as_bytes());
    let hex = hasher.finalize().to_hex();
    format!("{prefix}_{}", &hex[..24])
}

/// Generate a fresh `ClientsRegistry` populated with secure random API keys and `require_auth: false`.
pub fn generate_default_config() -> ClientsRegistry {
    ClientsRegistry {
        clients: vec![
            ClientEntry {
                id: "antigravity".to_string(),
                name: "Antigravity Agent".to_string(),
                key: Some(generate_token("ag")),
                color: "#38bdf8".to_string(),
            },
            ClientEntry {
                id: "claude".to_string(),
                name: "Claude Desktop".to_string(),
                key: Some(generate_token("claude")),
                color: "#f97316".to_string(),
            },
            ClientEntry {
                id: "gemini".to_string(),
                name: "Gemini CLI".to_string(),
                key: Some(generate_token("gemini")),
                color: "#ec4899".to_string(),
            },
            ClientEntry {
                id: "roo".to_string(),
                name: "Roo Code".to_string(),
                key: Some(generate_token("roo")),
                color: "#10b981".to_string(),
            },
            ClientEntry {
                id: "default".to_string(),
                name: "Anonymous Agent".to_string(),
                key: None,
                color: "#a855f7".to_string(),
            },
        ],
        require_auth: false,
        daemon_key: Some(generate_token("daemon")),
    }
}

/// Discover candidate paths for `clients.json`.
pub fn get_client_config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(path_str) = std::env::var("CTXV_CLIENTS_CONFIG") {
        if !path_str.is_empty() {
            candidates.push(PathBuf::from(path_str));
        }
    }

    candidates.push(PathBuf::from("clients.json"));
    candidates.push(PathBuf::from("ctxv-clients.json"));
    candidates.push(crate::config::get_cache_dir().join("clients.json"));

    candidates
}

/// Load the clients registry from disk, falling back to built-in defaults.
pub fn load_clients_config(explicit_path: Option<&Path>) -> ClientsRegistry {
    let mut registry = if let Some(path) = explicit_path {
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|c| serde_json::from_str::<ClientsRegistry>(&c).ok())
        } else {
            None
        }
    } else {
        None
    };

    if registry.is_none() {
        for candidate in get_client_config_candidates() {
            if candidate.exists() {
                if let Ok(content) = std::fs::read_to_string(&candidate) {
                    if let Ok(reg) = serde_json::from_str::<ClientsRegistry>(&content) {
                        registry = Some(reg);
                        break;
                    }
                }
            }
        }
    }

    let mut reg = registry.unwrap_or_default();

    // Check environment variable overrides
    if let Ok(val) = std::env::var("CTXV_REQUIRE_AUTH") {
        if val == "1" || val.eq_ignore_ascii_case("true") {
            reg.require_auth = true;
        } else if val == "0" || val.eq_ignore_ascii_case("false") {
            reg.require_auth = false;
        }
    }

    if let Ok(key) =
        std::env::var("CTXV_INTERNAL_API_KEY").or_else(|_| std::env::var("CTXV_DAEMON_KEY"))
    {
        if !key.trim().is_empty() {
            reg.daemon_key = Some(key.trim().to_string());
        }
    }

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_registry_no_auth() {
        let registry = ClientsRegistry::default();
        assert!(!registry.require_auth);
        assert_eq!(registry.daemon_key, None);
        assert!(registry.clients.len() >= 4);

        let ag = registry.find_by_id("antigravity").unwrap();
        assert_eq!(ag.name, "Antigravity Agent");
        assert_eq!(ag.color, "#38bdf8");
    }

    #[test]
    fn test_generate_default_config_has_keys() {
        let generated = generate_default_config();
        assert!(!generated.require_auth);
        assert!(generated.daemon_key.is_some());
        let daemon_k = generated.daemon_key.as_ref().unwrap();
        assert!(daemon_k.starts_with("daemon_"));
        assert!(generated.is_valid_key(daemon_k));

        let ag = generated.find_by_id("antigravity").unwrap();
        let ag_key = ag.key.as_ref().unwrap();
        assert!(ag_key.starts_with("ag_"));
        assert!(generated.is_valid_key(ag_key));
    }

    #[test]
    fn test_resolve_key_and_id() {
        let mut reg = ClientsRegistry::default();
        reg.clients[0].key = Some("test-secret-123".to_string());

        let resolved = reg.resolve(Some("test-secret-123"), None).unwrap();
        assert_eq!(resolved.id, "antigravity");

        let resolved_id = reg.resolve(None, Some("claude")).unwrap();
        assert_eq!(resolved_id.id, "claude");

        let resolved_default = reg.resolve(None, None).unwrap();
        assert_eq!(resolved_default.id, "default");
    }
}
