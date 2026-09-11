---
title: "MCP Client & IDE Integration"
description: "Configuring Cursor, Claude Desktop, Antigravity IDE, Windsurf, Zed, and VS Code."
category: "building"
status: "active"
tags: ["mcp", "cursor", "claude", "antigravity", "windsurf", "zed", "vscode", "configuration"]
related:
  - "[[docs/architecture/building/index]]"
  - "[[docs/architecture/building/installation]]"
  - "[[docs/concepts/progressive-disclosure/tool-profiles]]"
  - "[[docs/architecture/implementation/mcp-transport]]"
---

# MCP Client & IDE Integration

`ctxvault` communicates natively over standard input/output (stdio JSON-RPC) and Server-Sent Events (HTTP SSE), adhering strictly to the Model Context Protocol specification.

* **Agent Auto-Installer**: [`crates/ctxvault-cli/src/installer/mod.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-cli/src/installer/mod.rs)
* **Stdio Transport**: [`crates/ctxvault-mcp/src/transport/stdio.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp/src/transport/stdio.rs)
* **HTTP SSE Transport**: [`crates/ctxvault-mcp/src/transport/http.rs`](file:///c:/dev/ctx/ctxvault/crates/ctxvault-mcp/src/transport/http.rs)

---

## 1. Automated Setup (`ctxvault install -y`)

`ctxvault` includes an auto-detection installer that finds configuration files for installed editors on your machine and registers `ctxvault`:

```bash
ctxvault install -y
```

This scans for:
* Cursor (`~/.cursor/mcp.json` or `%USERPROFILE%\.cursor\mcp.json`)
* Claude Desktop (`claude_desktop_config.json`)
* Claude Code (`config.json`)
* Antigravity IDE & Gemini CLI (`mcp_config.json`)
* Windsurf (`~/.codeium/windsurf/mcp_config.json`)
* VS Code (`settings.json` / cline / roo)
* Zed (`settings.json`)

---

## 2. Manual Configurations

### Cursor (`.cursor/mcp.json`)
Create or edit `.cursor/mcp.json` in your repository root:
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--corpus", "${workspaceFolder}",
        "--sync",
        "--profile", "all"
      ]
    }
  }
}
```

### Claude Desktop (`claude_desktop_config.json`)
Location:
* **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
* **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--corpus", "C:\\path\\to\\your\\workspace",
        "--sync"
      ]
    }
  }
}
```

### Antigravity IDE & Gemini CLI
Add to `mcp_config.json` or IDE Settings:
```json
{
  "mcpServers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": [
        "--corpus", "${workspaceRoot}",
        "--sync"
      ]
    }
  }
}
```

### Zed Editor (`~/.config/zed/settings.json`)
```json
{
  "context_servers": {
    "ctxvault": {
      "command": "ctxvault",
      "args": ["--corpus", "/path/to/project", "--sync"]
    }
  }
}
```

---

## Role-Based Profiles (`--profile`)

Control which tools are advertised to your agent:
* `--profile scout`: Exposes only read-only retrieval tools (`search`, `get_snippet`, `read_file`, `list_notes`, `status`). Ideal for lightweight code exploration.
* `--profile analysis`: Adds graph traversal and validation tools (`graph_match`, `graph_communities`, `validate`, `list_templates`).
* `--profile all` (default): Exposes all 17 authoritative tools including mutating writes (`write_note`, `delete_note`, `move_note`, `sync_corpus`).
