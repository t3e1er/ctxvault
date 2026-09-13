//! File skeleton map chunking for `IndexMode::Skeleton`.
//!
//! Aggregates per-symbol code chunks into bounded, file-level public API skeleton
//! chunks for embedding. See ADR-018: File Skeleton Map Chunking for Skeleton Indexing Mode.

use std::collections::{HashMap, HashSet};

use ctxvault_common::types::{Chunk, ChunkEmbedPolicy};

use crate::engine::PendingChunk;

/// Aggregate per-symbol skeleton texts into 1–K file-level PendingChunks.
///
/// Groups symbols by top-level container scope (struct/trait/class + its methods),
/// then packs groups greedily into `max_tokens`-sized chunks. Each chunk gets
/// `embed_policy = ChunkEmbedPolicy::Anchor` and represents the file's public API skeleton.
///
/// The `chunk_index` of each `PendingChunk` is the first symbol's chunk_index in the group
/// (used as a file-level handle for VectorMeta storage).
pub fn build_file_skeleton_chunks(
    rel_path: &str,
    chunks: &[Chunk],
    max_tokens: usize,
) -> Vec<PendingChunk> {
    // 1. Filter to only Anchor-policy chunks
    let anchor_chunks: Vec<&Chunk> =
        chunks.iter().filter(|c| c.embed_policy == ChunkEmbedPolicy::Anchor).collect();

    if anchor_chunks.is_empty() {
        return Vec::new();
    }

    // 2. Build file header: path + language
    let header = build_skeleton_header(rel_path, chunks);
    let header_tokens = estimate_tokens(&header);

    // 3. Group anchor chunks by container scope
    let groups = group_by_container_scope(&anchor_chunks);

    // 4. Greedy pack groups into max_tokens-sized PendingChunks
    let mut result = Vec::new();
    let mut current_text = header.clone();
    let mut current_first_chunk_index = anchor_chunks[0].chunk_index;
    let mut current_token_count = header_tokens;

    for (container_name, group) in groups {
        for (i, chunk) in group.iter().enumerate() {
            let item_text = chunk.skeleton_text.as_deref().unwrap_or(&chunk.text).trim();
            if item_text.is_empty() {
                continue;
            }
            let item_tokens = estimate_tokens(item_text);

            // If adding this item exceeds max_tokens and we already have items in this chunk, flush
            if current_token_count + item_tokens > max_tokens && current_token_count > header_tokens
            {
                result.push(PendingChunk {
                    doc_path: rel_path.to_string(),
                    chunk_index: current_first_chunk_index,
                    text: current_text,
                    embed_policy: ChunkEmbedPolicy::Anchor,
                    modality: "code".to_string(),
                });

                // New chunk with header and optional continuation breadcrumb
                current_text = header.clone();
                if !container_name.is_empty() && i > 0 {
                    current_text.push_str(&format!("\n// (continued {container_name})"));
                }
                current_token_count = estimate_tokens(&current_text);
                current_first_chunk_index = chunk.chunk_index;
            }

            current_text.push_str("\n\n");
            current_text.push_str(item_text);
            current_token_count += item_tokens;
        }
    }

    // Flush final chunk
    if current_token_count > header_tokens {
        result.push(PendingChunk {
            doc_path: rel_path.to_string(),
            chunk_index: current_first_chunk_index,
            text: current_text,
            embed_policy: ChunkEmbedPolicy::Anchor,
            modality: "code".to_string(),
        });
    }

    result
}

/// Build file header comment containing path and language if available.
pub fn build_skeleton_header(rel_path: &str, chunks: &[Chunk]) -> String {
    let lang = chunks.iter().find_map(|c| c.language.as_deref()).unwrap_or("");
    if lang.is_empty() {
        format!("// File: {}", rel_path)
    } else {
        format!("// File: {} | {}", rel_path, lang)
    }
}

/// Rough byte heuristic for token estimation (4 characters per token).
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Group chunks by top-level container scope, preserving source ordering.
pub fn group_by_container_scope<'a>(chunks: &[&'a Chunk]) -> Vec<(String, Vec<&'a Chunk>)> {
    // Collect all containers that appear as prefixes with " > "
    let mut container_prefixes = HashSet::new();
    for c in chunks {
        if let Some(ref scope) = c.scope_path {
            if let Some((prefix, _)) = scope.split_once(" > ") {
                container_prefixes.insert(prefix.trim().to_string());
            }
        }
    }

    let mut groups: Vec<(String, Vec<&'a Chunk>)> = Vec::new();
    let mut group_map: HashMap<String, usize> = HashMap::new();

    for &c in chunks {
        let container_key = match c.scope_path.as_deref() {
            Some(scope) => {
                let trimmed = scope.trim();
                if let Some((prefix, _)) = trimmed.split_once(" > ") {
                    prefix.trim().to_string()
                } else if container_prefixes.contains(trimmed) {
                    trimmed.to_string()
                } else {
                    String::new()
                }
            }
            None => String::new(),
        };

        if let Some(&idx) = group_map.get(&container_key) {
            groups[idx].1.push(c);
        } else {
            let idx = groups.len();
            group_map.insert(container_key.clone(), idx);
            groups.push((container_key, vec![c]));
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_chunk(index: usize, scope: &str, skeleton: &str) -> Chunk {
        Chunk::new("src/auth.rs", index, "full body", 0, 100)
            .with_code_metadata("rust", scope, 1, 10)
            .with_embed_policy(ChunkEmbedPolicy::Anchor)
            .with_skeleton_text(skeleton)
    }

    #[test]
    fn test_file_skeleton_chunks_groups_by_scope() {
        let chunks = vec![
            make_test_chunk(0, "AuthService", "pub struct AuthService;"),
            make_test_chunk(1, "AuthService > login", "pub fn login(&self);"),
            make_test_chunk(2, "AuthService > logout", "pub fn logout(&self);"),
            make_test_chunk(3, "helper", "pub fn helper();"),
            make_test_chunk(4, "Token", "pub struct Token;"),
            make_test_chunk(5, "Token > verify", "pub fn verify(&self);"),
        ];

        let pending = build_file_skeleton_chunks("src/auth.rs", &chunks, 512);
        // All skeletons easily fit into 512 tokens -> exactly 1 file-level PendingChunk
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].doc_path, "src/auth.rs");
        assert_eq!(pending[0].chunk_index, 0);
        assert_eq!(pending[0].embed_policy, ChunkEmbedPolicy::Anchor);
        assert_eq!(pending[0].modality, "code");

        // Verify the rendered content contains all signatures
        assert!(pending[0].text.contains("// File: src/auth.rs | rust"));
        assert!(pending[0].text.contains("pub struct AuthService;"));
        assert!(pending[0].text.contains("pub fn login(&self);"));
        assert!(pending[0].text.contains("pub fn logout(&self);"));
        assert!(pending[0].text.contains("pub fn helper();"));
        assert!(pending[0].text.contains("pub struct Token;"));
        assert!(pending[0].text.contains("pub fn verify(&self);"));
    }

    #[test]
    fn test_file_skeleton_chunks_respects_token_limit() {
        // Create 10 chunks, each with ~100 characters (~25 tokens)
        let mut chunks = Vec::new();
        for i in 0..10 {
            let sig =
                format!("pub fn very_long_function_signature_number_{i}() -> Result<(), Error>;");
            chunks.push(make_test_chunk(i, "Service > fn", &sig));
        }

        // Set max_tokens to a small value (e.g. 50 tokens)
        let pending = build_file_skeleton_chunks("src/lib.rs", &chunks, 50);
        assert!(pending.len() > 1, "Should split into multiple pending chunks");
        for p in &pending {
            assert_eq!(p.doc_path, "src/lib.rs");
            assert_eq!(p.embed_policy, ChunkEmbedPolicy::Anchor);
        }
    }

    #[test]
    fn test_empty_chunks_returns_empty() {
        let pending = build_file_skeleton_chunks("src/empty.rs", &[], 512);
        assert!(pending.is_empty());

        let graph_only_chunks = vec![Chunk::new("src/tests.rs", 0, "fn test_foo() {}", 0, 20)
            .with_embed_policy(ChunkEmbedPolicy::GraphOnly)];
        let pending_graph = build_file_skeleton_chunks("src/tests.rs", &graph_only_chunks, 512);
        assert!(pending_graph.is_empty());
    }
}
