//! Configuration CLI command handlers for `ctxvault config` (get/set/list).
//!
//! Backed by `${CTXV_CACHE_DIR}/config.toml`.

use ctxvault_common::config::{get_config_path, load_global_config, save_global_config, IndexMode};

/// Handle `ctxvault config list`.
pub fn handle_config_list() -> anyhow::Result<()> {
    let cfg = load_global_config();
    let path = get_config_path();
    println!("# Configuration: {}", path.display());
    println!("{}", toml::to_string_pretty(&cfg)?);
    Ok(())
}

/// Handle `ctxvault config get <key>`.
pub fn handle_config_get(key: &str) -> anyhow::Result<()> {
    let cfg = load_global_config();
    match key {
        "auto_index" => println!("{}", cfg.auto_index),
        "index_mode" => println!("{:?}", cfg.index_mode),
        "idle_timeout_mins" => println!("{}", cfg.idle_timeout_mins),
        "log_level" => println!("{}", cfg.log_level),
        "cache_dir" => println!("{}", cfg.cache_dir.as_deref().unwrap_or("")),
        _ => anyhow::bail!("unknown config key: '{}'", key),
    }
    Ok(())
}

/// Handle `ctxvault config set <key> <val>`.
pub fn handle_config_set(key: &str, val: &str) -> anyhow::Result<()> {
    let mut cfg = load_global_config();
    match key {
        "auto_index" => {
            cfg.auto_index = val.parse::<bool>().map_err(|_| {
                anyhow::anyhow!(
                    "invalid boolean value for auto_index: '{}' (expected true/false)",
                    val
                )
            })?;
        }
        "index_mode" => {
            cfg.index_mode = match val.to_lowercase().as_str() {
                "full" => IndexMode::Full,
                "docs-embed" | "docsembed" => IndexMode::DocsEmbed,
                "fast" => IndexMode::Fast,
                _ => anyhow::bail!(
                    "invalid index_mode: '{}' (expected full, docs-embed, or fast)",
                    val
                ),
            };
        }
        "idle_timeout_mins" => {
            cfg.idle_timeout_mins = val.parse::<u64>().map_err(|_| {
                anyhow::anyhow!("invalid integer value for idle_timeout_mins: '{}'", val)
            })?;
        }
        "log_level" => {
            cfg.log_level = val.to_string();
        }
        "cache_dir" => {
            cfg.cache_dir = if val.is_empty() { None } else { Some(val.to_string()) };
        }
        _ => anyhow::bail!("unknown config key: '{}'", key),
    }

    save_global_config(&cfg)?;
    let path = get_config_path();
    println!("[OK] Set {} = {} in {}", key, val, path.display());
    Ok(())
}
