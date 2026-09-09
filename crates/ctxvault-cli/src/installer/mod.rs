//! Auto-configuration and installation of ctxvault for coding agents.
//!
//! Detects installed coding agents across platform standard paths, configures
//! `mcpServers` with a zero-arg `ctxvault` launcher entry, and optionally
//! auto-populates up-to-date agent steering rules.

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// Default authoritative steering rule content for AI agents interacting with ctxvault.
pub const CTXVAULT_STEERING_RULE: &str = r#"# ctxvault MCP Steering Protocol

You have access to the `ctxvault` Model Context Protocol (MCP) server (17 authoritative tools). Follow these rules when querying or modifying knowledge and source code:

1. Retrieval Strategy & Turn 1 Snippets:
   - Use `search` with `mode="hybrid"` as your default exploratory discovery tool (3-way RRF across BM25 lexical, ONNX dense vectors, and graph).
   - `search` automatically returns Turn 1 inline text and code snippets (`snippets: 3` by default) along with graph affordances (`calls_in`, `calls_out`, `implements`, `imports`, `wikilinks_in`). Work directly from these snippets whenever possible to avoid unnecessary round-trips.
   - Use `search` with `mode="bm25"` when searching for exact identifier names, error strings, struct symbols, or CLI flags.
   - Use `search` with `mode="semantic"` for abstract natural-language concepts.
   - Use `search_related` for Personalized PageRank expansion around known seed notes or symbols.

2. Bounded Code & Document Inspection (Progressive Disclosure):
   - Tier 1: Survey results via `search` (inspect handles, Turn 1 snippets, and graph affordances).
   - Tier 2: Fetch exact symbol definitions or bounded doc chunks with `get_snippet(symbol="...")` or `get_snippet(path="...", chunk_index=0)`.
   - Tier 3: Call `read_file` with explicit line bounds (`start_line`, `end_line`) only when exhaustive context is required. Do NOT dump entire large files into context.

3. Graph Navigation with Cypher-Lite (`graph_match`):
   - Call `graph_match` using linear Cypher-Lite ASCII patterns with cycle guards:
     - Trace call graphs: `(:CodeSymbol {name: "MyFunction"})-[:calls*1..2]->(target)`
     - Trace implementations: `(source)-[:implements]->(target)`
     - Trace doc ancestry: `(:DocNode {path: "adrs/001.md"})-[:derived_from*1..3]->(target)`
   - Filter by `edge_class`: `"code"`, `"structural"`, `"semantic"`, `"crossmodal"`, or `"hybrid"`.
   - Use `graph_communities(view="architecture")` for high-level architectural component maps.

4. Note Creation & Schema Discipline:
   - Before authoring a new document or ADR, call `list_templates` to discover available schemas.
   - Author or update notes via `write_note(path="...", mode="create"|"overwrite"|"append")`.
   - Always run `validate(path="...")` immediately after creating or modifying a note to ensure zero schema errors.

5. Principle 3 Knowledge Crystallization:
   - When resolving complex architectural questions, subtle bugs, or incident resolutions, crystallize findings into permanent notes using `write_note` (with `derived_from` frontmatter) and verify lineage via `graph_match`.
"#;

/// Configuration target for a coding agent.
#[derive(Debug, Clone)]
pub struct AgentTarget {
    /// Human-readable agent name.
    pub name: &'static str,
    /// Absolute path to configuration file.
    pub path: PathBuf,
}

/// Steering rule target for a coding agent.
#[derive(Debug, Clone)]
pub struct RuleTarget {
    /// Human-readable target name.
    pub name: &'static str,
    /// Absolute path to the rule file.
    pub path: PathBuf,
}

/// Discovered agent installation and configuration status.
#[derive(Debug, Default)]
pub struct InstallSummary {
    /// Agents successfully configured with MCP servers.
    pub configured: Vec<String>,
    /// Agents detected during dry-run.
    pub dry_run_detected: Vec<String>,
    /// Agents skipped because app is not installed.
    pub skipped: Vec<String>,
    /// Steering rule files successfully created or updated.
    pub rules_configured: Vec<String>,
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

/// Detect steering rule targets for coding agents and workspace.
pub fn detect_rule_targets(workspace_dir: Option<&Path>) -> Vec<RuleTarget> {
    let mut targets = Vec::new();

    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    let home_path = PathBuf::from(&home);

    // 1. Antigravity IDE global rules
    let antigravity_rules =
        home_path.join(".gemini").join("antigravity-ide").join("rules").join("ctxvault.md");
    targets.push(RuleTarget { name: "Antigravity Global Rules", path: antigravity_rules });

    // 2. Gemini CLI global rules
    let gemini_rules = home_path.join(".gemini").join("config").join("rules").join("ctxvault.md");
    targets.push(RuleTarget { name: "Gemini CLI Global Rules", path: gemini_rules });

    // 3. Cursor global rules
    let cursor_rules = home_path.join(".cursor").join("rules").join("ctxvault.mdc");
    targets.push(RuleTarget { name: "Cursor Global Rules", path: cursor_rules });

    // 4. Claude Code home instructions
    let claude_rules = home_path.join(".claude").join("CLAUDE.md");
    targets.push(RuleTarget { name: "Claude Code Home Rules", path: claude_rules });

    // 5. Workspace-specific rules if workspace_dir provided
    if let Some(ws) = workspace_dir {
        targets.push(RuleTarget { name: "Workspace GEMINI.md", path: ws.join("GEMINI.md") });
        targets.push(RuleTarget { name: "Workspace .cursorrules", path: ws.join(".cursorrules") });
        targets
            .push(RuleTarget { name: "Workspace .windsurfrules", path: ws.join(".windsurfrules") });
    }

    targets
}

/// Run auto-configuration across detected agents and optionally populate steering rules.
pub fn run_install(
    install_dir: Option<&Path>,
    dry_run: bool,
    _auto_confirm: bool,
    install_rules: bool,
    workspace_dir: Option<&Path>,
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

    // Auto-populate steering rules if requested
    if install_rules {
        let rule_targets = detect_rule_targets(workspace_dir);
        for rt in rule_targets {
            let parent_exists = rt.path.parent().map(|p| p.exists()).unwrap_or(false);
            if !parent_exists && workspace_dir.is_none() {
                // If it's a global agent path and the parent agent directory does not exist, skip
                continue;
            }

            let path_display = rt.path.display().to_string();
            if dry_run {
                summary
                    .dry_run_detected
                    .push(format!("{} -> Would write steering rule to: {}", rt.name, path_display));
            } else {
                if let Some(parent) = rt.path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&rt.path, CTXVAULT_STEERING_RULE)?;
                summary.rules_configured.push(format!("{} [{}]", rt.name, path_display));
            }
        }
    }

    Ok(summary)
}
