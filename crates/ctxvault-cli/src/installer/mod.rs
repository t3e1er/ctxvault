//! Auto-configuration and installation of ctxvault for coding agents.
//!
//! Detects installed coding agents across platform standard paths and configures
//! `mcpServers` with a zero-arg `ctxvault` launcher entry.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration target for a coding agent.
#[derive(Debug, Clone)]
pub struct AgentTarget {
    /// Human-readable agent name.
    pub name: &'static str,
    /// Absolute path to configuration file.
    pub path: PathBuf,
}

/// Discovered agent installation and configuration status.
#[derive(Debug, Default)]
pub struct InstallSummary {
    /// Agents successfully configured.
    pub configured: Vec<String>,
    /// Agents detected during dry-run.
    pub dry_run_detected: Vec<String>,
    /// Agents skipped because app is not installed.
    pub skipped: Vec<String>,
}

/// Detect configuration locations for all supported coding agents on the current OS.
pub fn detect_agents() -> Vec<AgentTarget> {
    let mut targets = Vec::new();

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let home_path = PathBuf::from(&home);

    #[cfg(windows)]
    let app_data = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_path.join("AppData").join("Roaming"));

    #[cfg(not(windows))]
    let config_dir = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_path.join(".config"));

    // 1. Antigravity / Gemini
    targets.push(AgentTarget {
        name: "Antigravity IDE",
        path: home_path.join(".gemini").join("antigravity-ide").join("mcp_config.json"),
    });
    targets.push(AgentTarget {
        name: "Gemini CLI / Extension",
        path: home_path.join(".gemini").join("config").join("mcp_config.json"),
    });

    // 2. Cursor
    #[cfg(windows)]
    {
        targets.push(AgentTarget {
            name: "Cursor (Global Settings)",
            path: app_data.join("Cursor").join("User").join("mcp.json"),
        });
        targets.push(AgentTarget {
            name: "Cursor (Roo/Cline MCP)",
            path: app_data
                .join("Cursor")
                .join("User")
                .join("globalStorage")
                .join("rooveterinaryinc.roo-cline")
                .join("settings")
                .join("cline_mcp_settings.json"),
        });
    }
    #[cfg(not(windows))]
    {
        targets.push(AgentTarget {
            name: "Cursor",
            path: config_dir.join("Cursor").join("User").join("mcp.json"),
        });
    }
    targets.push(AgentTarget {
        name: "Cursor (User Profile)",
        path: home_path.join(".cursor").join("mcp.json"),
    });

    // 3. Claude Desktop & Claude Code
    #[cfg(windows)]
    {
        targets.push(AgentTarget {
            name: "Claude Desktop",
            path: app_data.join("Claude").join("claude_desktop_config.json"),
        });
    }
    #[cfg(not(windows))]
    {
        targets.push(AgentTarget {
            name: "Claude Desktop",
            path: home_path.join(".claude").join("claude_desktop_config.json"),
        });
    }
    targets.push(AgentTarget { name: "Claude Code CLI", path: home_path.join(".claude.json") });

    // 4. Windsurf
    targets.push(AgentTarget {
        name: "Windsurf",
        path: home_path.join(".codeium").join("windsurf").join("mcp_config.json"),
    });

    // 5. VS Code / Copilot
    #[cfg(windows)]
    {
        targets.push(AgentTarget {
            name: "VS Code User MCP",
            path: app_data.join("Code").join("User").join("mcp.json"),
        });
        targets.push(AgentTarget {
            name: "GitHub Copilot Chat MCP",
            path: app_data
                .join("Code")
                .join("User")
                .join("globalStorage")
                .join("github.copilot-chat")
                .join("mcp.json"),
        });
    }
    #[cfg(not(windows))]
    {
        targets.push(AgentTarget {
            name: "VS Code User MCP",
            path: config_dir.join("Code").join("User").join("mcp.json"),
        });
    }

    // 6. Zed
    #[cfg(windows)]
    {
        targets.push(AgentTarget { name: "Zed", path: app_data.join("Zed").join("settings.json") });
    }
    #[cfg(not(windows))]
    {
        targets
            .push(AgentTarget { name: "Zed", path: config_dir.join("zed").join("settings.json") });
    }

    targets
}

/// Run auto-configuration across detected agents.
pub fn run_install(
    install_dir: Option<&Path>,
    dry_run: bool,
    _auto_confirm: bool,
) -> anyhow::Result<InstallSummary> {
    let binary_command = if let Some(dir) = install_dir {
        let exe = if cfg!(windows) { "ctxvault.exe" } else { "ctxvault" };
        dir.join(exe).to_string_lossy().to_string()
    } else {
        "ctxvault".to_string()
    };

    let targets = detect_agents();
    let mut summary = InstallSummary::default();

    for target in targets {
        let file_exists = target.path.exists();
        let parent_exists = target.path.parent().map(|p| p.exists()).unwrap_or(false);

        // Only configure if either the config file already exists or its parent app directory exists.
        if !file_exists && !parent_exists {
            summary.skipped.push(format!("{} (app not detected)", target.name));
            continue;
        }

        let mut root_val: Value = if file_exists {
            match fs::read_to_string(&target.path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or(json!({})),
                Err(_) => json!({}),
            }
        } else {
            json!({})
        };

        if !root_val.is_object() {
            root_val = json!({});
        }

        let root_map = root_val.as_object_mut().unwrap();

        // Ensure mcpServers object exists
        let servers = root_map.entry("mcpServers".to_string()).or_insert_with(|| json!({}));

        if let Some(servers_map) = servers.as_object_mut() {
            servers_map.insert(
                "ctxvault".to_string(),
                json!({
                    "command": binary_command,
                    "args": []
                }),
            );
        }

        let target_display = target.path.display().to_string();

        if dry_run {
            summary.dry_run_detected.push(format!(
                "{} -> Would configure zero-arg ctxvault at: {}",
                target.name, target_display
            ));
        } else {
            if let Some(parent) = target.path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let pretty_json = serde_json::to_string_pretty(&root_val)?;
            fs::write(&target.path, pretty_json)?;
            summary.configured.push(format!("{} [{}]", target.name, target_display));
        }
    }

    Ok(summary)
}
