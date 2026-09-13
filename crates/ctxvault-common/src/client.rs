//! Client tracking, authentication, and visual identity configuration.
//!
//! Provides multi-agent tracking and color association for MCP sessions and
//! the 3D GraphView visualizer. Supports optional local network client-key auth.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

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
        }
    }
}

impl ClientsRegistry {
    /// Find a client entry by its secret key.
    pub fn find_by_key(&self, key: &str) -> Option<&ClientEntry> {
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
        self.find_by_id("default")
            .or_else(|| self.clients.first())
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
    if let Some(path) = explicit_path {
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Ok(registry) = serde_json::from_str::<ClientsRegistry>(&content) {
                    return registry;
                }
            }
        }
    }

    for candidate in get_client_config_candidates() {
        if candidate.exists() {
            if let Ok(content) = std::fs::read_to_string(&candidate) {
                if let Ok(registry) = serde_json::from_str::<ClientsRegistry>(&content) {
                    return registry;
                }
            }
        }
    }

    ClientsRegistry::default()
}
