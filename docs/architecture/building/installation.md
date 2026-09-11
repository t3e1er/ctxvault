---
title: "Installation & Standalone Binaries"
description: "Installing native precompiled release binaries and the bundled ONNX embedding model sidecar."
category: "building"
status: "active"
tags: ["install", "binaries", "powershell", "curl", "sidecar", "embeddings"]
related:
  - "[[docs/architecture/building/index]]"
  - "[[docs/architecture/building/build-from-source]]"
  - "[[docs/architecture/building/client-setup]]"
---

# Installation & Standalone Binaries

`ctxvault` release archives bundle both the native `ctxvault` executable and the local 768-dimensional ONNX embedding model sidecar (`jina-embeddings-v2-base-code`). No external Python environment, C-compiler, or Docker container is required.

---

## One-Command Shell Installers

### Windows (PowerShell)
Run in PowerShell (as your standard user account):
```powershell
irm https://raw.githubusercontent.com/t3e1er/ctxvault/master/install.ps1 | iex
```
* **Install Location**: `%LOCALAPPDATA%\Programs\ctxvault\`
* **Sidecar Location**: `%LOCALAPPDATA%\Programs\ctxvault\models\jina-embeddings-v2-base-code\`
* **Path Registration**: Automatically adds `ctxvault` to your user `PATH`.

### macOS & Linux (Bash / Zsh)
Run in terminal:
```bash
curl -fsSL https://raw.githubusercontent.com/t3e1er/ctxvault/master/install.sh | sh
```
* **Install Location**: `~/.local/bin/ctxvault` (or `/usr/local/bin` if root)
* **Sidecar Location**: `~/.local/share/ctxvault/models/jina-embeddings-v2-base-code/`

---

## Manual Binary Download

Direct release archives are published for every tagged version on GitHub Releases:
* **Windows**: `ctxvault-vX.Y.Z-x86_64-pc-windows-msvc.zip`
* **Linux x86_64**: `ctxvault-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
* **Linux aarch64**: `ctxvault-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz`
* **macOS Apple Silicon**: `ctxvault-vX.Y.Z-aarch64-apple-darwin.tar.gz`

### Sidecar Layout
The executable expects the embedding model sidecar in one of three locations:
1. An environment variable: `CTX_MODELS_DIR=/path/to/models`
2. A `models/` directory adjacent to the `ctxvault` executable:
   ```
   ctxvault-dir/
   ├── ctxvault (or ctxvault.exe)
   └── models/
       └── jina-embeddings-v2-base-code/
           ├── model.onnx
           └── tokenizer.json
   ```
3. A relative `../models/` directory (used during development and `cargo test`).

---

## Verification

Confirm installation and inspect system capabilities:
```bash
ctxvault --version
ctxvault status --scope indexing
```
If GPU acceleration is active, the status output displays the detected DirectX 12 Compute adapter (e.g. NVIDIA GeForce, AMD Radeon, or Intel Arc).
