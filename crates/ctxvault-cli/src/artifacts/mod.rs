//! Portable team graph artifact export and import (.ctxvault/vault.tar.zst).
//!
//! Enables zero-reindex onboarding across teams by packaging the derived indices
//! (SQLite metadata, Tantivy BM25, and Petgraph graph) into a compressed archive.

use std::fs;
use std::path::{Path, PathBuf};

/// Export the corpus index into a compressed team sharing artifact (.ctxvault/vault.tar.zst).
pub fn export_artifact(
    index_dir: &Path,
    repo_root: &Path,
    output_path: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if !index_dir.exists() {
        anyhow::bail!("Index directory '{}' does not exist", index_dir.display());
    }

    // Flush SQLite WAL checkpoint before archiving
    let db_path = index_dir.join("meta.db");
    if db_path.exists() {
        if let Ok(store) = ctxvault_core::persistence::Store::open(&db_path) {
            let _ = store.checkpoint();
        }
    }

    let dest = if let Some(out) = output_path {
        out.to_path_buf()
    } else {
        repo_root.join(".ctxvault").join("vault.tar.zst")
    };

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::create(&dest)?;
    let zstd_writer = zstd::stream::Encoder::new(file, 3)?;
    let mut tar_builder = tar::Builder::new(zstd_writer);

    tar_builder.append_dir_all(".", index_dir)?;
    let zstd_writer = tar_builder.into_inner()?;
    zstd_writer.finish()?;

    Ok(dest)
}

/// Import a compressed team sharing artifact into the destination index directory.
pub fn import_artifact(src_path: &Path, dest_index_dir: &Path) -> anyhow::Result<PathBuf> {
    if !src_path.exists() {
        anyhow::bail!("Artifact archive '{}' does not exist", src_path.display());
    }

    fs::create_dir_all(dest_index_dir)?;

    let file = fs::File::open(src_path)?;
    let zstd_reader = zstd::stream::Decoder::new(file)?;
    let mut archive = tar::Archive::new(zstd_reader);

    archive.unpack(dest_index_dir)?;

    Ok(dest_index_dir.to_path_buf())
}
