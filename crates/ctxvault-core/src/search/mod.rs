//! Search strategies: BM25, semantic (vector), hybrid, graph, related, multihop.

use std::collections::{HashMap, HashSet};

use ctxvault_common::ports::{EmbeddingProvider, GraphStore, TextIndex, VectorStore};
use ctxvault_common::types::{Modality, ScoreBreakdown, SearchResult};
use ctxvault_common::Result;

/// Classify a result path as passing a [`Modality`] filter using the set of
/// known code node keys.
///
/// A path is treated as code if it appears in `code_paths` (scope_paths, code
/// file paths, and `<corpus>::scope_path` keys derived from the code-symbol
/// catalog); otherwise it is documentation. [`Modality::Both`] accepts every
/// path.
pub fn path_matches_modality(path: &str, modality: Modality, code_paths: &HashSet<String>) -> bool {
    match modality {
        Modality::Both => true,
        Modality::Code => code_paths.contains(path),
        Modality::Docs => !code_paths.contains(path),
    }
}

/// Simple BM25 keyword search, restricted to the requested [`Modality`].
pub fn search_bm25(
    bm25: &impl TextIndex,
    query: &str,
    limit: usize,
    modality: Modality,
) -> Result<Vec<SearchResult>> {
    bm25.search_with_modality(query, limit, modality)
}

/// Semantic vector search using embedding similarity.
///
/// Embeds the query text, then searches the vector index for nearest neighbors.
/// Returns results ranked by cosine similarity.
///
/// - `vector_index`: The HNSW vector index to search.
/// - `embedder`: The embedding model to encode the query.
/// - `query`: The natural language query to embed and search.
/// - `limit`: Maximum results to return.
/// - `doc_level_only`: If true, only search document-level embeddings (broad mode).
pub fn search_semantic(
    vector_index: &impl VectorStore,
    embedder: &impl EmbeddingProvider,
    query: &str,
    limit: usize,
    doc_level_only: bool,
    modality: Modality,
) -> Result<Vec<SearchResult>> {
    // 1. Embed the query.
    let query_embedding = embedder.embed_query(query)?;

    // 2. Search vector index.
    let vector_results = vector_index.search(&query_embedding, limit, doc_level_only, modality)?;

    // 3. Convert to SearchResult.
    let results: Vec<SearchResult> = vector_results
        .into_iter()
        .map(|vr| {
            SearchResult::new(vr.doc_path, vr.score)
                .with_chunk_index(vr.chunk_index)
                .with_score_components(ScoreBreakdown {
                    bm25: 0.0,
                    vector: vr.score,
                    graph_boost: 0.0,
                    graph_hops: None,
                })
        })
        .collect();

    Ok(results)
}

/// Semantic vector search using a pre-computed query embedding.
///
/// Use this when you already have the query embedding (avoids redundant embedding).
pub fn search_semantic_with_embedding(
    vector_index: &impl VectorStore,
    query_embedding: &[f32],
    limit: usize,
    doc_level_only: bool,
    modality: Modality,
) -> Result<Vec<SearchResult>> {
    let vector_results = vector_index.search(query_embedding, limit, doc_level_only, modality)?;

    let results: Vec<SearchResult> = vector_results
        .into_iter()
        .map(|vr| {
            SearchResult::new(vr.doc_path, vr.score)
                .with_chunk_index(vr.chunk_index)
                .with_score_components(ScoreBreakdown {
                    bm25: 0.0,
                    vector: vr.score,
                    graph_boost: 0.0,
                    graph_hops: None,
                })
        })
        .collect();

    Ok(results)
}

/// Dual-level semantic search with depth parameter.
///
/// Implements the LightRAG-inspired dual-level retrieval:
/// - **Precise**: chunk-level vectors only (specific passages)
/// - **Broad**: document-level vectors only (thematically connected docs)
/// - **Adaptive**: both levels merged with Reciprocal Rank Fusion (default)
///
/// The `depth` parameter controls which level(s) to search.
pub fn search_semantic_dual(
    vector_index: &impl VectorStore,
    embedder: &impl EmbeddingProvider,
    query: &str,
    limit: usize,
    depth: ctxvault_common::types::SearchDepth,
    modality: Modality,
) -> Result<Vec<SearchResult>> {
    use ctxvault_common::types::SearchDepth;

    // Embed the query.
    let query_embedding = embedder.embed_query(query)?;

    match depth {
        SearchDepth::Precise => {
            // Chunk-level only.
            search_semantic_with_embedding(vector_index, &query_embedding, limit, false, modality)
        }
        SearchDepth::Broad => {
            // Document-level only.
            search_semantic_with_embedding(vector_index, &query_embedding, limit, true, modality)
        }
        SearchDepth::Adaptive => {
            // Both levels, merged with RRF.
            let chunk_results = search_semantic_with_embedding(
                vector_index,
                &query_embedding,
                limit * 2,
                false,
                modality,
            )?;
            let doc_results = search_semantic_with_embedding(
                vector_index,
                &query_embedding,
                limit * 2,
                true,
                modality,
            )?;

            // RRF fusion of both result sets.
            let fused = rrf_fuse(&[&chunk_results, &doc_results], limit);
            Ok(fused)
        }
    }
}

/// Reciprocal Rank Fusion: merges multiple ranked lists into one.
///
/// RRF score = sum over all lists of: 1 / (k + rank_in_list)
/// where k = 60 (standard constant from the RRF paper).
fn rrf_fuse(result_lists: &[&[SearchResult]], limit: usize) -> Vec<SearchResult> {
    const K: f64 = 60.0;

    // Accumulate RRF scores per document path.
    let mut rrf_scores: HashMap<String, (f64, Option<String>, Option<usize>, ScoreBreakdown)> =
        HashMap::new();

    for list in result_lists {
        for (rank, result) in list.iter().enumerate() {
            let rrf_contribution = 1.0 / (K + rank as f64 + 1.0);

            let entry = rrf_scores.entry(result.path.clone()).or_insert_with(|| {
                (
                    0.0,
                    result.snippet.clone(),
                    result.chunk_index,
                    ScoreBreakdown { bm25: 0.0, vector: 0.0, graph_boost: 0.0, graph_hops: None },
                )
            });
            entry.0 += rrf_contribution;

            // Accumulate the vector component from the original score.
            if let Some(ref components) = result.score_components {
                if components.vector > entry.3.vector {
                    entry.3.vector = components.vector;
                }
            }
        }
    }

    // Build results sorted by RRF score.
    let mut results: Vec<SearchResult> = rrf_scores
        .into_iter()
        .map(|(path, (score, snippet, chunk_index, components))| {
            SearchResult::new(path, score)
                .with_snippet(snippet)
                .with_chunk_index(chunk_index)
                .with_score_components(components)
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

/// Cross-corpus Reciprocal Rank Fusion: merges per-corpus ranked lists into one
/// ranking, preserving each result's source-corpus tag and rich fields.
///
/// Each input list is `(corpus_name, results)`, already ranked descending. RRF is
/// applied over each list (K = 60, contribution `1 / (K + rank + 1)`). Results are
/// keyed by `(corpus, path)` so the same path appearing in two different corpora is
/// preserved as two distinct hits, each tagged with its origin corpus. The source
/// result's snippet, chunk index, entity kind, language, lineage, and score
/// components are carried through. The fused list is sorted by RRF score descending
/// and truncated to `limit`.
pub fn rrf_fuse_cross_corpus(
    tagged_lists: &[(String, Vec<SearchResult>)],
    limit: usize,
) -> Vec<SearchResult> {
    const K: f64 = 60.0;

    // Accumulate fused RRF score per (corpus, path); keep the first-seen rich result.
    let mut fused: HashMap<(String, String), (f64, SearchResult)> = HashMap::new();

    for (corpus_name, list) in tagged_lists {
        for (rank, result) in list.iter().enumerate() {
            let contribution = 1.0 / (K + rank as f64 + 1.0);
            let key = (corpus_name.clone(), result.path.clone());

            let entry = fused.entry(key).or_insert_with(|| {
                let tagged = result.clone().with_corpus(Some(corpus_name.clone()));
                (0.0, tagged)
            });
            entry.0 += contribution;
        }
    }

    // Apply fused score and collect.
    let mut results: Vec<SearchResult> = fused
        .into_values()
        .map(|(score, mut result)| {
            result.score = score;
            result
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.corpus.cmp(&b.corpus))
            .then_with(|| a.path.cmp(&b.path))
    });
    results.truncate(limit);
    results
}

/// Enrich search results with structural lineage metadata from the knowledge graph.
pub fn enrich_results_with_lineage(results: &mut [SearchResult], graph: &impl GraphStore) {
    for result in results.iter_mut() {
        if result.lineage.is_none() {
            result.lineage = graph.extract_lineage_for_node(&result.path);
        }
    }
}

/// Hybrid search: seeds from BM25, then boosts scores based on graph proximity.
///
/// Strategy:
/// 1. SEED: BM25(query, limit * 3)
/// 2. EXPAND: For each seed, BFS over edges to depth `graph_depth`
/// 3. RANK: RRF fusion of BM25 rank + graph proximity rank (k=60)
/// 4. RETURN: Top `limit` results
///
/// Note: This is the BM25+graph variant. For true 3-signal hybrid (BM25+vector+graph),
/// use `search_hybrid_full`.
pub fn search_hybrid(
    bm25: &impl TextIndex,
    graph: &impl GraphStore,
    query: &str,
    limit: usize,
    graph_depth: usize,
    edge_type_filter: Option<&[String]>,
    edge_class_filter: Option<ctxvault_common::config::EdgeClass>,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<SearchResult>> {
    const RRF_K: f64 = 60.0;

    // 1. Get BM25 seeds (over-fetch to allow graph reranking), modality-filtered.
    let bm25_results = bm25.search_with_modality(query, limit * 3, modality)?;

    if bm25_results.is_empty() {
        return Ok(Vec::new());
    }

    // 2. Build BM25 rank map: path -> (raw_score, rank_1based, snippet, chunk_index).
    let mut bm25_info: HashMap<String, (f64, usize, Option<String>, Option<usize>)> =
        HashMap::new();
    for (rank, r) in bm25_results.iter().enumerate() {
        let _ = bm25_info.entry(r.path.clone()).or_insert((
            r.score,
            rank + 1,
            r.snippet.clone(),
            r.chunk_index,
        ));
    }

    // 3. Graph expansion: BFS from each BM25 seed, accumulate proximity scores.
    let mut graph_boost_map: HashMap<String, (f64, usize)> = HashMap::new(); // path -> (total_boost, min_hops)

    for r in &bm25_results {
        let neighbors =
            graph.traverse_bfs(&r.path, graph_depth, edge_type_filter, edge_class_filter);
        for (neighbor_path, hops) in neighbors {
            if hops == 0 {
                continue;
            }
            let boost = 1.0 / (hops as f64);
            let entry = graph_boost_map.entry(neighbor_path).or_insert((0.0, hops));
            entry.0 += boost;
            if hops < entry.1 {
                entry.1 = hops;
            }
        }
    }

    // 4. Rank graph-discovered nodes by proximity score.
    let mut graph_ranked: Vec<(String, f64, usize)> =
        graph_boost_map.into_iter().map(|(path, (boost, hops))| (path, boost, hops)).collect();
    graph_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut graph_rank_map: HashMap<String, (f64, usize, usize)> = HashMap::new(); // path -> (boost, min_hops, rank_1based)
    for (rank, (path, boost, hops)) in graph_ranked.iter().enumerate() {
        let _ = graph_rank_map.insert(path.clone(), (*boost, *hops, rank + 1));
    }

    // 5. Collect all unique paths from both signals.
    let all_paths: std::collections::HashSet<String> =
        bm25_info.keys().chain(graph_rank_map.keys()).cloned().collect();

    // 6. RRF fusion: combine BM25 rank and graph rank.
    let mut results: Vec<SearchResult> = all_paths
        .into_iter()
        .map(|path| {
            let (bm25_score, bm25_rank, snippet, chunk_index) =
                bm25_info.get(&path).cloned().unwrap_or((0.0, 0, None, None));

            let (graph_boost, min_hops, graph_rank) =
                graph_rank_map.get(&path).copied().unwrap_or((0.0, 0, 0));

            let bm25_rrf = if bm25_rank > 0 { 1.0 / (RRF_K + bm25_rank as f64) } else { 0.0 };
            let graph_rrf = if graph_rank > 0 { 1.0 / (RRF_K + graph_rank as f64) } else { 0.0 };

            let final_score = bm25_rrf + graph_rrf;

            SearchResult::new(path, final_score)
                .with_snippet(snippet)
                .with_chunk_index(chunk_index)
                .with_score_components(ScoreBreakdown {
                    bm25: bm25_score,
                    vector: 0.0,
                    graph_boost,
                    graph_hops: if min_hops > 0 { Some(min_hops) } else { None },
                })
        })
        .collect();

    // 7. Sort descending by RRF score, with deterministic tie-breaking on direct match then path.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_direct = a.score_components.as_ref().map_or(0.0, |c| c.bm25 + c.vector);
                let b_direct = b.score_components.as_ref().map_or(0.0, |c| c.bm25 + c.vector);
                b_direct.partial_cmp(&a_direct).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    // Modality filter: graph-expanded paths may introduce the other modality.
    results.retain(|r| path_matches_modality(&r.path, modality, code_paths));
    results.truncate(limit);
    enrich_results_with_lineage(&mut results, graph);

    Ok(results)
}

/// True hybrid search: fuses BM25 + Vector + Graph via Reciprocal Rank Fusion.
///
/// Strategy (from architecture doc):
/// 1. SEED: BM25(query, limit*3) ∪ Vector(query, limit*3)
/// 2. EXPAND: For each seed node, BFS over typed edges to depth D
/// 3. RANK: RRF fusion of:
///    - BM25 score rank
///    - Vector cosine similarity rank
///    - Graph proximity boost (1/hop_distance)
/// 4. RETURN: Top K results with scores + traversal path
///
/// If `query_embedding` is None, falls back to BM25+graph only.
pub fn search_hybrid_full(
    bm25: &impl TextIndex,
    vector_index: &impl VectorStore,
    graph: &impl GraphStore,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
    graph_depth: usize,
    edge_type_filter: Option<&[String]>,
    edge_class_filter: Option<ctxvault_common::config::EdgeClass>,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<SearchResult>> {
    tracing::debug!("hybrid search: vector results from anchor embeddings, BM25 from full corpus");
    const RRF_K: f64 = 60.0;

    // 1. Get BM25 seeds (modality-filtered).
    let bm25_results = bm25.search_with_modality(query, limit * 3, modality)?;

    // 2. Get vector seeds (if embedding available), modality-filtered.
    let vector_results = if let Some(emb) = query_embedding {
        vector_index.search(emb, limit * 3, false, modality)?
    } else {
        Vec::new()
    };

    // If both are empty, no results.
    if bm25_results.is_empty() && vector_results.is_empty() {
        return Ok(Vec::new());
    }

    // 3. Build RRF scores from BM25 ranked list.
    let mut rrf_map: HashMap<String, (f64, f64, f64, Option<String>, Option<usize>, usize)> =
        HashMap::new(); // path -> (rrf_total, bm25_score, vector_score, snippet, chunk, min_hops)

    for (rank, r) in bm25_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = rrf_map.entry(r.path.clone()).or_insert((
            0.0,
            0.0,
            0.0,
            r.snippet.clone(),
            r.chunk_index,
            0,
        ));
        entry.0 += rrf_score;
        entry.1 = r.score; // raw BM25 score
    }

    // 4. Add RRF scores from vector ranked list.
    for (rank, vr) in vector_results.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry =
            rrf_map.entry(vr.doc_path.clone()).or_insert((0.0, 0.0, 0.0, None, vr.chunk_index, 0));
        entry.0 += rrf_score;
        entry.2 = vr.score; // cosine similarity
    }

    // 5. Graph expansion: BFS from all seed docs to add graph boost.
    let seed_paths: Vec<String> = rrf_map.keys().cloned().collect();
    let mut graph_boost_map: HashMap<String, (f64, usize)> = HashMap::new();

    for seed_path in &seed_paths {
        let neighbors =
            graph.traverse_bfs(seed_path, graph_depth, edge_type_filter, edge_class_filter);
        for (neighbor_path, hops) in neighbors {
            if hops == 0 {
                continue;
            }
            let boost = 1.0 / (hops as f64);
            let entry = graph_boost_map.entry(neighbor_path).or_insert((0.0, hops));
            entry.0 += boost;
            if hops < entry.1 {
                entry.1 = hops;
            }
        }
    }

    // 6. Add graph boost as a third signal via RRF-style scoring.
    //    Sort graph-discovered nodes by boost, then assign RRF rank scores.
    let mut graph_ranked: Vec<(String, f64, usize)> =
        graph_boost_map.into_iter().map(|(path, (boost, hops))| (path, boost, hops)).collect();
    graph_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (rank, (path, boost, hops)) in graph_ranked.iter().enumerate() {
        let rrf_score = 1.0 / (RRF_K + rank as f64 + 1.0);
        let entry = rrf_map.entry(path.clone()).or_insert((0.0, 0.0, 0.0, None, None, 0));
        entry.0 += rrf_score;
        if *hops > 0 && (entry.5 == 0 || *hops < entry.5) {
            entry.5 = *hops;
        }
        // Store the raw graph boost for the breakdown.
        // We use a trick: store it by adding to entry if needed.
        let _ = boost; // used indirectly via rank
    }

    // 7. Build final results.
    let mut results: Vec<SearchResult> = rrf_map
        .into_iter()
        .map(|(path, (rrf_total, bm25_score, vector_score, snippet, chunk_index, min_hops))| {
            SearchResult::new(path, rrf_total)
                .with_snippet(snippet)
                .with_chunk_index(chunk_index)
                .with_score_components(ScoreBreakdown {
                    bm25: bm25_score,
                    vector: vector_score,
                    graph_boost: if min_hops > 0 { 1.0 / (min_hops as f64) } else { 0.0 },
                    graph_hops: if min_hops > 0 { Some(min_hops) } else { None },
                })
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_direct = a.score_components.as_ref().map_or(0.0, |c| c.bm25 + c.vector);
                let b_direct = b.score_components.as_ref().map_or(0.0, |c| c.bm25 + c.vector);
                b_direct.partial_cmp(&a_direct).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    // Modality filter: graph-expanded paths may introduce the other modality.
    results.retain(|r| path_matches_modality(&r.path, modality, code_paths));
    results.truncate(limit);
    enrich_results_with_lineage(&mut results, graph);

    Ok(results)
}

/// Full scoring breakdown search — returns detailed explanations per result.
///
/// Runs the 3-signal hybrid search (BM25 + vector + graph) and provides
/// per-result breakdown of each signal's raw score, rank, and RRF contribution.
pub fn search_explain(
    bm25: &impl TextIndex,
    vector_index: &impl VectorStore,
    graph: &impl GraphStore,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
    graph_depth: usize,
    edge_type_filter: Option<&[String]>,
    edge_class_filter: Option<ctxvault_common::config::EdgeClass>,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<ctxvault_common::types::SearchExplanation>> {
    use ctxvault_common::types::{GraphExplanation, SearchExplanation, SignalExplanation};

    const RRF_K: f64 = 60.0;

    // 1. Get BM25 results (modality-filtered).
    let bm25_results = bm25.search_with_modality(query, limit * 3, modality)?;

    // 2. Get vector results (if embedding available), modality-filtered.
    let vector_results = if let Some(emb) = query_embedding {
        vector_index.search(emb, limit * 3, false, modality)?
    } else {
        Vec::new()
    };

    // 3. Build per-path BM25 signal info (score + rank).
    let mut bm25_info: HashMap<String, (f64, usize, Option<String>, Option<usize>)> =
        HashMap::new(); // path -> (score, rank_1based, snippet, chunk_index)
    for (rank, r) in bm25_results.iter().enumerate() {
        let _ = bm25_info.entry(r.path.clone()).or_insert((
            r.score,
            rank + 1,
            r.snippet.clone(),
            r.chunk_index,
        ));
    }

    // 4. Build per-path vector signal info.
    let mut vector_info: HashMap<String, (f64, usize)> = HashMap::new(); // path -> (score, rank_1based)
    for (rank, vr) in vector_results.iter().enumerate() {
        let _ = vector_info.entry(vr.doc_path.clone()).or_insert((vr.score, rank + 1));
    }

    // 5. Graph expansion from all seeds.
    let all_seed_paths: std::collections::HashSet<String> =
        bm25_info.keys().chain(vector_info.keys()).cloned().collect();

    let mut graph_boost_map: HashMap<String, (f64, usize)> = HashMap::new();
    for seed_path in &all_seed_paths {
        let neighbors =
            graph.traverse_bfs(seed_path, graph_depth, edge_type_filter, edge_class_filter);
        for (neighbor_path, hops) in neighbors {
            if hops == 0 {
                continue;
            }
            let boost = 1.0 / (hops as f64);
            let entry = graph_boost_map.entry(neighbor_path).or_insert((0.0, hops));
            entry.0 += boost;
            if hops < entry.1 {
                entry.1 = hops;
            }
        }
    }

    // Sort graph entries by boost to assign ranks.
    let mut graph_ranked: Vec<(String, f64, usize)> =
        graph_boost_map.iter().map(|(path, &(boost, hops))| (path.clone(), boost, hops)).collect();
    graph_ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut graph_rank_map: HashMap<String, (f64, usize, usize)> = HashMap::new(); // path -> (boost, hops, rank_1based)
    for (rank, (path, boost, hops)) in graph_ranked.iter().enumerate() {
        let _ = graph_rank_map.insert(path.clone(), (*boost, *hops, rank + 1));
    }

    // 6. Collect all unique paths.
    let all_paths: std::collections::HashSet<String> =
        bm25_info.keys().chain(vector_info.keys()).chain(graph_rank_map.keys()).cloned().collect();

    // 7. Build explanations with RRF scores.
    let mut explanations: Vec<SearchExplanation> = all_paths
        .into_iter()
        .map(|path| {
            let (bm25_score, bm25_rank, snippet, chunk_index) =
                bm25_info.get(&path).cloned().unwrap_or((0.0, 0, None, None));

            let (vector_score, vector_rank) = vector_info.get(&path).copied().unwrap_or((0.0, 0));

            let (graph_boost, graph_hops, graph_rank) =
                graph_rank_map.get(&path).copied().unwrap_or((0.0, 0, 0));

            let bm25_rrf = if bm25_rank > 0 { 1.0 / (RRF_K + bm25_rank as f64) } else { 0.0 };
            let vector_rrf = if vector_rank > 0 { 1.0 / (RRF_K + vector_rank as f64) } else { 0.0 };
            let graph_rrf = if graph_rank > 0 { 1.0 / (RRF_K + graph_rank as f64) } else { 0.0 };

            let final_score = bm25_rrf + vector_rrf + graph_rrf;

            SearchExplanation {
                path,
                final_score,
                bm25: SignalExplanation {
                    raw_score: bm25_score,
                    rank: bm25_rank,
                    rrf_contribution: bm25_rrf,
                },
                vector: SignalExplanation {
                    raw_score: vector_score,
                    rank: vector_rank,
                    rrf_contribution: vector_rrf,
                },
                graph: GraphExplanation {
                    boost: graph_boost,
                    min_hops: if graph_hops > 0 { Some(graph_hops) } else { None },
                    rank: graph_rank,
                    rrf_contribution: graph_rrf,
                },
                snippet,
                chunk_index,
            }
        })
        .collect();

    explanations.sort_by(|a, b| {
        b.final_score
            .partial_cmp(&a.final_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let a_direct = a.bm25.raw_score + a.vector.raw_score;
                let b_direct = b.bm25.raw_score + b.vector.raw_score;
                b_direct.partial_cmp(&a_direct).unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.path.cmp(&b.path))
    });
    // Modality filter: graph-expanded paths may introduce the other modality.
    explanations.retain(|e| path_matches_modality(&e.path, modality, code_paths));
    explanations.truncate(limit);

    Ok(explanations)
}

/// Graph search: find nodes matching query text in BM25, then traverse graph from matches.
/// Returns nodes reachable from query matches via typed edges.
pub fn search_graph(
    bm25: &impl TextIndex,
    graph: &impl GraphStore,
    query: &str,
    limit: usize,
    max_depth: usize,
    edge_type_filter: Option<&[String]>,
    edge_class_filter: Option<ctxvault_common::config::EdgeClass>,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<SearchResult>> {
    // 1. Find seed nodes via BM25 (top 5). Seeds themselves are unrestricted so
    //    traversal can bridge modalities; the final results are modality-filtered.
    let seeds = bm25.search(query, 5)?;

    if seeds.is_empty() {
        return Ok(Vec::new());
    }

    // 2. BFS from each seed, accumulating scores.
    let mut score_map: HashMap<String, (f64, usize)> = HashMap::new(); // path -> (total_score, min_hops)

    for seed in &seeds {
        let neighbors =
            graph.traverse_bfs(&seed.path, max_depth, edge_type_filter, edge_class_filter);
        for (path, hops) in neighbors {
            if hops == 0 {
                continue;
            }
            let score = 1.0 / (hops as f64);
            let entry = score_map.entry(path).or_insert((0.0, hops));
            entry.0 += score;
            if hops < entry.1 {
                entry.1 = hops;
            }
        }
    }

    // Remove seeds from results (we want discovered nodes, not the seeds themselves).
    for seed in &seeds {
        let _ = score_map.remove(&seed.path);
    }

    // 3. Build results, sort by score, take top `limit`.
    let mut results: Vec<SearchResult> = score_map
        .into_iter()
        .map(|(path, (score, min_hops))| {
            SearchResult::new(path, score).with_score_components(ScoreBreakdown {
                bm25: 0.0,
                vector: 0.0,
                graph_boost: score,
                graph_hops: Some(min_hops),
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.retain(|r| path_matches_modality(&r.path, modality, code_paths));
    results.truncate(limit);
    enrich_results_with_lineage(&mut results, graph);

    Ok(results)
}

/// Related search: given seed document paths, find documents most related to them.
/// Uses multi-source BFS approximation of Personalized PageRank.
///
/// Strategy:
/// 1. For each seed, BFS to depth 3
/// 2. Accumulate score for each neighbor: `1.0 / (hop_distance * seeds.len())`
/// 3. Remove seeds from results
/// 4. Sort by accumulated score, take top `limit`
pub fn search_related(
    graph: &impl GraphStore,
    seeds: &[String],
    limit: usize,
    _damping: f64,
    _iterations: usize,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<SearchResult>> {
    if seeds.is_empty() {
        return Ok(Vec::new());
    }

    let num_seeds = seeds.len() as f64;
    let mut score_map: HashMap<String, (f64, usize)> = HashMap::new(); // path -> (accumulated_score, min_hops)

    for seed in seeds {
        let neighbors = graph.traverse_bfs(seed, 3, None, None);
        for (path, hops) in neighbors {
            if hops == 0 {
                continue;
            }
            let score = 1.0 / (hops as f64 * num_seeds);
            let entry = score_map.entry(path).or_insert((0.0, hops));
            entry.0 += score;
            if hops < entry.1 {
                entry.1 = hops;
            }
        }
    }

    // Remove seeds from results.
    for seed in seeds {
        let _ = score_map.remove(seed);
    }

    // Build results, sort, truncate.
    let mut results: Vec<SearchResult> = score_map
        .into_iter()
        .map(|(path, (score, min_hops))| {
            SearchResult::new(path, score).with_score_components(ScoreBreakdown {
                bm25: 0.0,
                vector: 0.0,
                graph_boost: score,
                graph_hops: Some(min_hops),
            })
        })
        .collect();

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.retain(|r| path_matches_modality(&r.path, modality, code_paths));
    results.truncate(limit);
    enrich_results_with_lineage(&mut results, graph);

    Ok(results)
}

/// Multi-hop query decomposition search.
///
/// Strategy:
/// 1. DECOMPOSE: Split query on connecting words into sub-concepts
/// 2. SEARCH: Run BM25 + vector for each sub-concept separately
/// 3. MERGE: RRF fusion across all sub-concept result lists
/// 4. BOOST: Documents appearing in multiple sub-concept results get score multiplier
/// 5. BRIDGE: Find structural paths between seed docs from different sub-concepts
///
/// Falls back to normal hybrid search if query cannot be decomposed.
pub fn search_multihop<E: EmbeddingProvider>(
    bm25: &impl TextIndex,
    vector_index: &impl VectorStore,
    graph: &impl GraphStore,
    embedder: Option<&E>,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: usize,
    graph_depth: usize,
    edge_type_filter: Option<&[String]>,
    modality: Modality,
    code_paths: &HashSet<String>,
) -> Result<Vec<SearchResult>> {
    use ctxvault_common::config::EdgeClass;

    const RRF_K: f64 = 60.0;

    // 1. Decompose query into sub-concepts.
    let sub_concepts = decompose_query(query);

    // If decomposition yields only one concept, fall back to hybrid_full with semantic edges.
    if sub_concepts.len() <= 1 {
        return search_hybrid_full(
            bm25,
            vector_index,
            graph,
            query,
            query_embedding,
            limit,
            graph_depth,
            edge_type_filter,
            Some(EdgeClass::Semantic),
            modality,
            code_paths,
        );
    }

    // 2. Search each sub-concept separately with BM25 + Vector.
    let mut all_result_lists: Vec<Vec<SearchResult>> = Vec::new();
    let mut concept_doc_sets: Vec<HashSet<String>> = Vec::new();

    for concept in &sub_concepts {
        let clean_concept =
            concept.trim_matches(|c: char| c == '?' || c == '.' || c == '!' || c == ',').trim();

        // BM25 search for this sub-concept (modality-filtered).
        let bm25_results = bm25.search_with_modality(clean_concept, limit * 2, modality)?;

        // Vector search for this sub-concept if embedder is available.
        let concept_list = if let Some(emb_model) = embedder {
            if let Ok(emb) = emb_model.embed_query(clean_concept) {
                let vec_res = vector_index.search(&emb, limit * 2, false, modality)?;
                let vec_search: Vec<SearchResult> = vec_res
                    .into_iter()
                    .map(|vr| {
                        SearchResult::new(vr.doc_path, vr.score).with_chunk_index(vr.chunk_index)
                    })
                    .collect();
                rrf_fuse(&[&bm25_results, &vec_search], limit * 2)
            } else {
                bm25_results
            }
        } else {
            bm25_results
        };

        // Track which docs appear for this concept.
        let doc_set: HashSet<String> = concept_list.iter().map(|r| r.path.clone()).collect();
        concept_doc_sets.push(doc_set);
        all_result_lists.push(concept_list);
    }

    // Also add the full-query BM25 results as another signal (modality-filtered).
    let full_bm25 = bm25.search_with_modality(query, limit * 2, modality)?;
    all_result_lists.push(full_bm25);

    // And full-query vector results if available.
    if let Some(emb) = query_embedding {
        let vector_results = vector_index.search(emb, limit * 2, false, modality)?;
        let vector_as_search: Vec<SearchResult> = vector_results
            .into_iter()
            .map(|vr| SearchResult::new(vr.doc_path, vr.score).with_chunk_index(vr.chunk_index))
            .collect();
        all_result_lists.push(vector_as_search);
    }

    // 3. Structural bridging: find docs on structural paths between concept seeds.
    // Take top seed from each concept and find structural paths between them.
    let mut bridge_docs: HashMap<String, f64> = HashMap::new();

    if concept_doc_sets.len() >= 2 {
        // Get top-3 seeds from each concept.
        let concept_seeds: Vec<Vec<String>> = all_result_lists
            .iter()
            .take(sub_concepts.len()) // Only concept-specific lists
            .map(|list| list.iter().take(3).map(|r| r.path.clone()).collect())
            .collect();

        // For each pair of concept seed sets, find structural paths.
        for i in 0..concept_seeds.len() {
            for j in (i + 1)..concept_seeds.len() {
                for seed_a in &concept_seeds[i] {
                    for seed_b in &concept_seeds[j] {
                        // Find shortest structural path between seeds.
                        if let Some(path_nodes) = graph.shortest_path(
                            seed_a,
                            seed_b,
                            edge_type_filter,
                            Some(EdgeClass::Structural),
                        ) {
                            // Intermediate nodes on the path are bridge documents.
                            for node in &path_nodes {
                                if node != seed_a && node != seed_b {
                                    let boost = 1.0 / (path_nodes.len() as f64);
                                    *bridge_docs.entry(node.clone()).or_insert(0.0) += boost;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Add bridge documents as a rank-normalized list to RRF fusion.
    if !bridge_docs.is_empty() {
        let mut sorted_bridges: Vec<(String, f64)> = bridge_docs.clone().into_iter().collect();
        sorted_bridges.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let bridge_as_search: Vec<SearchResult> = sorted_bridges
            .into_iter()
            .map(|(path, score)| SearchResult::new(path, score))
            .collect();
        all_result_lists.push(bridge_as_search);
    }

    // 4. RRF fusion across all result lists with weights.
    let mut rrf_scores: HashMap<String, (f64, Option<String>, Option<usize>)> = HashMap::new();

    for (list_idx, list) in all_result_lists.iter().enumerate() {
        let weight = if list_idx < sub_concepts.len() {
            1.0 // Sub-concept results get full weight
        } else if list_idx < sub_concepts.len() + 2 {
            0.5 // Full-query BM25 / Vector get standard support weight
        } else {
            0.2 // Structural bridge docs get subtle support weight
        };

        for (rank, result) in list.iter().enumerate() {
            let rrf_contribution = weight / (RRF_K + rank as f64 + 1.0);
            let entry = rrf_scores.entry(result.path.clone()).or_insert((
                0.0,
                result.snippet.clone(),
                result.chunk_index,
            ));
            entry.0 += rrf_contribution;
        }
    }

    // 5. Multi-concept boost: multiply score by number of concepts a doc appears in.
    for (path, (score, _, _)) in rrf_scores.iter_mut() {
        let concept_count =
            concept_doc_sets.iter().filter(|set| set.contains(path.as_str())).count();
        if concept_count > 1 {
            *score *= concept_count as f64;
        }
    }

    // 6. Build final results, sort, truncate.
    let mut results: Vec<SearchResult> = rrf_scores
        .into_iter()
        .map(|(path, (score, snippet, chunk_index))| {
            let is_bridge = bridge_docs.contains_key(&path);
            SearchResult::new(path, score)
                .with_snippet(snippet)
                .with_chunk_index(chunk_index)
                .with_score_components(ScoreBreakdown {
                    bm25: 0.0,
                    vector: 0.0,
                    graph_boost: if is_bridge { 1.0 } else { 0.0 },
                    graph_hops: None,
                })
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });
    // Modality filter: structural bridge nodes may introduce the other modality.
    results.retain(|r| path_matches_modality(&r.path, modality, code_paths));
    results.truncate(limit);
    enrich_results_with_lineage(&mut results, graph);

    Ok(results)
}

/// Decompose a complex query into sub-concepts by splitting on connecting words.
///
/// Returns a vector of sub-concept strings. If no splitting is possible,
/// returns a single-element vector with the original query.
pub fn decompose_query(query: &str) -> Vec<String> {
    // Connecting patterns to split on (order matters — try longer patterns first).
    let patterns = [
        " relate to ",
        " related to ",
        " relates to ",
        " connect to ",
        " connects to ",
        " connected to ",
        " connection between ",
        " relationship between ",
        " between ",
        " through ",
        " connect ",
        " and ",
        " to ",
        " from ",
    ];

    let query_clean = strip_question_prefix(query);

    // Try each pattern.
    for pattern in &patterns {
        if let Some(pos) = query_clean.to_lowercase().find(pattern) {
            let left = query_clean[..pos].trim();
            let right = query_clean[pos + pattern.len()..].trim();

            let left_clean = strip_question_prefix(left);
            let right_clean = strip_question_prefix(right);

            if left_clean.len() >= 3 && right_clean.len() >= 3 && left_clean != "the path" {
                let mut concepts = vec![left_clean.to_string()];
                let right_parts = decompose_query(right_clean);
                for part in right_parts {
                    if part != "the path" && part.len() >= 3 {
                        concepts.push(part);
                    }
                }
                return concepts;
            }
        }
    }

    // No decomposition possible — return the whole query (stripped of question prefix).
    if query_clean.len() >= 3 {
        vec![query_clean.to_string()]
    } else {
        vec![query.to_string()]
    }
}

/// Strip common question prefixes like "How do", "What is", etc.
fn strip_question_prefix(s: &str) -> &str {
    let prefixes = [
        "what is the path from ",
        "what is the path between ",
        "what is the path through ",
        "path from ",
        "how does ",
        "how do ",
        "how can ",
        "how is ",
        "what is ",
        "what are ",
        "what does ",
        "what do ",
        "what ",
        "how ",
        "why does ",
        "why do ",
        "why is ",
        "where does ",
        "where do ",
    ];

    let lower = s.to_lowercase();
    for prefix in &prefixes {
        if lower.starts_with(prefix) {
            return &s[prefix.len()..];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctxvault_common::types::{Chunk, EdgeProvenance};

    use crate::graph::KnowledgeGraph;
    use crate::index::BM25Index;

    /// Helper to create a simple chunk.
    fn make_chunk(doc_path: &str, index: usize, text: &str) -> Chunk {
        Chunk::new(doc_path, index, text, 0, text.len())
    }

    /// An empty code-paths classifier set (all paths classify as docs).
    fn no_code() -> HashSet<String> {
        HashSet::new()
    }

    /// Set up a BM25 index with some test documents.
    /// Documents are designed so that specific queries yield predictable top results.
    fn setup_bm25() -> BM25Index {
        let mut index = BM25Index::open_in_memory().unwrap();

        // rust.md mentions "systems programming" heavily — unique to this doc.
        let chunks = [
            make_chunk("notes/rust.md", 0, "Rust is a systems programming language. Systems programming in Rust focuses on safety and performance for systems-level code"),
            make_chunk("notes/async.md", 0, "Async concurrency uses futures and the tokio runtime for non-blocking IO"),
            make_chunk("notes/python.md", 0, "Python is a dynamic interpreted scripting language popular for data science and automation. Python scripting is easy to learn"),
            make_chunk("notes/ml.md", 0, "Machine learning uses neural networks and gradient descent for classification"),
            make_chunk("notes/web.md", 0, "Web development uses frameworks like actix-web and axum for HTTP servers"),
        ];

        index
            .add_document(
                "notes/rust.md",
                Some("Systems Programming"),
                &["rust".into(), "systems".into()],
                &chunks[0..1],
            )
            .unwrap();
        index
            .add_document("notes/async.md", Some("Async IO"), &["async".into()], &chunks[1..2])
            .unwrap();
        index
            .add_document(
                "notes/python.md",
                Some("Python Scripting"),
                &["python".into()],
                &chunks[2..3],
            )
            .unwrap();
        index
            .add_document("notes/ml.md", Some("Neural Networks"), &["ml".into()], &chunks[3..4])
            .unwrap();
        index
            .add_document("notes/web.md", Some("HTTP Servers"), &["web".into()], &chunks[4..5])
            .unwrap();
        index.commit().unwrap();

        index
    }

    /// Set up a knowledge graph with connections between test docs.
    fn setup_graph() -> KnowledgeGraph {
        let mut graph = KnowledgeGraph::new();

        // Rust cluster: rust -> async, rust -> web
        graph.add_edge(
            "notes/rust.md",
            "notes/async.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );
        graph.add_edge(
            "notes/rust.md",
            "notes/web.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );
        graph.add_edge(
            "notes/async.md",
            "notes/web.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );

        // Python cluster: python -> ml
        graph.add_edge(
            "notes/python.md",
            "notes/ml.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );

        // Cross-cluster link: ml -> rust (ML uses Rust for performance)
        graph.add_edge(
            "notes/ml.md",
            "notes/rust.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );

        graph
    }

    #[test]
    fn test_search_bm25() {
        let index = setup_bm25();

        let results = search_bm25(&index, "systems programming", 10, Modality::Both).unwrap();
        assert!(!results.is_empty(), "Expected BM25 results for 'systems programming'");

        // rust.md is the only doc that mentions "systems programming" heavily.
        assert_eq!(results[0].path, "notes/rust.md");

        // Scores should be in descending order.
        for window in results.windows(2) {
            assert!(window[0].score >= window[1].score);
        }
    }

    #[test]
    fn test_search_hybrid() {
        let index = setup_bm25();
        let graph = setup_graph();

        // Search for "systems programming" — hybrid should boost graph-connected nodes.
        let results = search_hybrid(
            &index,
            &graph,
            "systems programming",
            10,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();
        assert!(!results.is_empty());

        // rust.md should be top (only doc with BM25 match for "systems programming").
        assert_eq!(results[0].path, "notes/rust.md");

        // Graph-connected nodes (async.md, web.md) should appear in results
        // because rust.md links to them, giving them graph_boost > 0.
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"notes/async.md"), "async.md should appear via graph boost");
        assert!(paths.contains(&"notes/web.md"), "web.md should appear via graph boost");

        // Check that graph-boosted results have score_components set.
        for r in &results {
            if r.path == "notes/async.md" || r.path == "notes/web.md" {
                let components = r.score_components.as_ref().unwrap();
                assert!(components.graph_boost > 0.0, "{} should have graph_boost > 0", r.path);
            }
        }
    }

    #[test]
    fn test_search_graph() {
        let index = setup_bm25();
        let graph = setup_graph();

        // Search for "Python scripting" — graph search finds nodes reachable from python.md.
        let results = search_graph(
            &index,
            &graph,
            "Python scripting",
            10,
            3,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();
        assert!(!results.is_empty());

        // python.md links to ml.md, and ml.md links to rust.md.
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"notes/ml.md"), "ml.md should be reachable from python.md");

        // ml.md should have higher score (1 hop) than rust.md (2 hops).
        let ml_result = results.iter().find(|r| r.path == "notes/ml.md");
        let rust_result = results.iter().find(|r| r.path == "notes/rust.md");

        if let (Some(ml), Some(rust)) = (ml_result, rust_result) {
            assert!(
                ml.score > rust.score,
                "ml.md (1 hop) should score higher than rust.md (2 hops)"
            );
        }

        // Seeds (python.md) should NOT appear in graph search results.
        assert!(!paths.contains(&"notes/python.md"), "seed should be excluded from results");
    }

    #[test]
    fn test_search_related() {
        let graph = setup_graph();

        // Find notes related to rust.md.
        let seeds = vec!["notes/rust.md".to_string()];
        let results =
            search_related(&graph, &seeds, 10, 0.85, 20, Modality::Both, &no_code()).unwrap();
        assert!(!results.is_empty());

        // rust.md links to async.md and web.md directly (1 hop).
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"notes/async.md"));
        assert!(paths.contains(&"notes/web.md"));

        // Seeds should not appear in results.
        assert!(!paths.contains(&"notes/rust.md"));

        // Direct neighbors (1 hop) should score higher than 2-hop neighbors.
        let async_result = results.iter().find(|r| r.path == "notes/async.md").unwrap();
        assert_eq!(async_result.score_components.as_ref().unwrap().graph_hops, Some(1));
    }

    #[test]
    fn test_search_related_multi_seed() {
        let graph = setup_graph();

        // Multiple seeds: rust.md and python.md.
        let seeds = vec!["notes/rust.md".to_string(), "notes/python.md".to_string()];
        let results =
            search_related(&graph, &seeds, 10, 0.85, 20, Modality::Both, &no_code()).unwrap();
        assert!(!results.is_empty());

        // ml.md is reachable from python.md (1 hop) and from rust.md via longer path.
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"notes/ml.md"));
        assert!(paths.contains(&"notes/async.md"));
        assert!(paths.contains(&"notes/web.md"));

        // Neither seed should appear.
        assert!(!paths.contains(&"notes/rust.md"));
        assert!(!paths.contains(&"notes/python.md"));
    }

    #[test]
    fn test_path_matches_modality_classifier() {
        let mut code_paths = HashSet::new();
        let _ = code_paths.insert("src/engine.rs".to_string());
        let _ = code_paths.insert("crate::search::Engine".to_string());

        // Both accepts everything.
        assert!(path_matches_modality("src/engine.rs", Modality::Both, &code_paths));
        assert!(path_matches_modality("notes/guide.md", Modality::Both, &code_paths));

        // Code keeps only code-set paths.
        assert!(path_matches_modality("src/engine.rs", Modality::Code, &code_paths));
        assert!(path_matches_modality("crate::search::Engine", Modality::Code, &code_paths));
        assert!(!path_matches_modality("notes/guide.md", Modality::Code, &code_paths));

        // Docs keeps only non-code-set paths.
        assert!(path_matches_modality("notes/guide.md", Modality::Docs, &code_paths));
        assert!(!path_matches_modality("src/engine.rs", Modality::Docs, &code_paths));
    }

    #[test]
    fn test_search_graph_modality_post_filter() {
        // Build a small graph: a doc seed linking to one doc node and one code node.
        let mut graph = KnowledgeGraph::new();
        graph.add_edge(
            "notes/design.md",
            "notes/related.md",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );
        graph.add_edge(
            "notes/design.md",
            "src/engine.rs",
            "documents",
            1.0,
            EdgeProvenance::DocumentsCode,
            ctxvault_common::config::EdgeClass::Structural,
        );

        // Seed matches the design doc via BM25.
        let mut index = BM25Index::open_in_memory().unwrap();
        let chunks = vec![make_chunk("notes/design.md", 0, "design document about the engine")];
        index.add_document("notes/design.md", Some("Design"), &[], &chunks).unwrap();
        index.commit().unwrap();

        // Classifier: only src/engine.rs is code.
        let mut code_paths = HashSet::new();
        let _ = code_paths.insert("src/engine.rs".to_string());

        // Code modality keeps only the code node.
        let code_results =
            search_graph(&index, &graph, "design", 10, 3, None, None, Modality::Code, &code_paths)
                .unwrap();
        let code_paths_out: Vec<&str> = code_results.iter().map(|r| r.path.as_str()).collect();
        assert!(code_paths_out.contains(&"src/engine.rs"));
        assert!(!code_paths_out.contains(&"notes/related.md"));

        // Docs modality keeps only the doc node.
        let doc_results =
            search_graph(&index, &graph, "design", 10, 3, None, None, Modality::Docs, &code_paths)
                .unwrap();
        let doc_paths_out: Vec<&str> = doc_results.iter().map(|r| r.path.as_str()).collect();
        assert!(doc_paths_out.contains(&"notes/related.md"));
        assert!(!doc_paths_out.contains(&"src/engine.rs"));

        // Both keeps everything discovered.
        let both_results =
            search_graph(&index, &graph, "design", 10, 3, None, None, Modality::Both, &code_paths)
                .unwrap();
        let both_paths_out: Vec<&str> = both_results.iter().map(|r| r.path.as_str()).collect();
        assert!(both_paths_out.contains(&"src/engine.rs"));
        assert!(both_paths_out.contains(&"notes/related.md"));
    }

    #[test]
    fn test_search_hybrid_empty_query() {
        let index = setup_bm25();
        let graph = setup_graph();

        // An empty or non-matching query should return empty results gracefully.
        // Note: tantivy may error on empty queries, so we test a non-matching term.
        let results = search_hybrid(
            &index,
            &graph,
            "xyznonexistent",
            10,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_hybrid_full_with_vectors() {
        use crate::vector_index::VectorIndex;

        let index = setup_bm25();
        let graph = setup_graph();

        // Set up a vector index with vectors for some documents.
        let mut vi = VectorIndex::new(384, 100, 200, 16);

        let make_vec = |seed: usize| -> Vec<f32> {
            let v: Vec<f32> =
                (0..384).map(|i| ((seed * 7 + i * 13) % 100) as f32 / 100.0).collect();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        };

        // Assign vectors: rust and python have similar vectors.
        let v_rust = make_vec(42);
        let v_python = make_vec(43); // very similar to rust
        let v_web = make_vec(200); // different

        vi.add(&v_rust, "notes/rust.md", Some(0), false, "docs").unwrap();
        vi.add(&v_python, "notes/python.md", Some(0), false, "docs").unwrap();
        vi.add(&v_web, "notes/web.md", Some(0), false, "docs").unwrap();

        // Search with the rust vector as query embedding.
        let results = search_hybrid_full(
            &index,
            &vi,
            &graph,
            "systems programming",
            Some(&v_rust),
            10,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();

        assert!(!results.is_empty());

        // rust.md should be top (has both BM25 and vector signal).
        assert_eq!(results[0].path, "notes/rust.md");

        // Results should have non-zero scores.
        for r in &results {
            assert!(r.score > 0.0, "{} should have score > 0", r.path);
        }

        // Graph-connected nodes should still appear.
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(
            paths.contains(&"notes/async.md") || paths.contains(&"notes/web.md"),
            "Graph-connected nodes should appear"
        );
    }

    #[test]
    fn test_search_hybrid_full_without_vectors() {
        use crate::vector_index::VectorIndex;

        let index = setup_bm25();
        let graph = setup_graph();
        let vi = VectorIndex::new_default(384); // empty vector index

        // Without query embedding, should still work (BM25+graph only).
        let results = search_hybrid_full(
            &index,
            &vi,
            &graph,
            "systems programming",
            None,
            10,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();

        assert!(!results.is_empty());

        // rust.md should appear (it's the BM25 match for "systems programming").
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"notes/rust.md"), "rust.md should appear in results");

        // Results should be sorted by descending score.
        for window in results.windows(2) {
            assert!(window[0].score >= window[1].score);
        }
    }

    #[test]
    fn test_search_related_empty_seeds() {
        let graph = setup_graph();

        let results =
            search_related(&graph, &[], 10, 0.85, 20, Modality::Both, &no_code()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_explain_returns_breakdowns() {
        use crate::vector_index::VectorIndex;

        let index = setup_bm25();
        let graph = setup_graph();
        let vi = VectorIndex::new_default(384); // no vectors, tests BM25+graph explain

        let explanations = search_explain(
            &index,
            &vi,
            &graph,
            "systems programming",
            None,
            10,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();

        assert!(!explanations.is_empty());

        // rust.md should appear since it matches "systems programming" in BM25.
        let rust_exp = explanations.iter().find(|e| e.path == "notes/rust.md");
        assert!(rust_exp.is_some(), "rust.md should appear in explanations");

        let rust = rust_exp.unwrap();
        // Should have BM25 signal.
        assert!(rust.bm25.raw_score > 0.0, "rust.md should have BM25 score");
        assert!(rust.bm25.rank > 0, "rust.md should have BM25 rank");
        assert!(rust.bm25.rrf_contribution > 0.0, "rust.md should have BM25 RRF contribution");

        // Vector signal should be zero (no embeddings).
        assert_eq!(rust.vector.raw_score, 0.0);
        assert_eq!(rust.vector.rank, 0);

        // Final score should be sum of RRF contributions.
        let expected_score =
            rust.bm25.rrf_contribution + rust.vector.rrf_contribution + rust.graph.rrf_contribution;
        assert!((rust.final_score - expected_score).abs() < 1e-10);

        // Results should be sorted by descending final_score.
        for window in explanations.windows(2) {
            assert!(window[0].final_score >= window[1].final_score);
        }
    }

    #[test]
    fn test_search_graph_with_edge_filter() {
        let mut graph = KnowledgeGraph::new();

        // Set up two types of edges.
        graph.add_edge(
            "A",
            "B",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );
        graph.add_edge(
            "A",
            "C",
            "SharedTag",
            0.5,
            EdgeProvenance::SharedTag,
            ctxvault_common::config::EdgeClass::Semantic,
        );
        graph.add_edge(
            "B",
            "D",
            "Link",
            1.0,
            EdgeProvenance::Wikilink,
            ctxvault_common::config::EdgeClass::Structural,
        );

        // Create a BM25 index that matches "A".
        let mut index = BM25Index::open_in_memory().unwrap();
        let chunks = vec![make_chunk("A", 0, "Alpha document about testing")];
        index.add_document("A", Some("Alpha"), &[], &chunks).unwrap();
        index.commit().unwrap();

        // With edge filter for "Link" only, should find B and D but not C.
        let filter = vec!["Link".to_string()];
        let results = search_graph(
            &index,
            &graph,
            "Alpha",
            10,
            3,
            Some(&filter),
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();

        assert!(paths.contains(&"B"));
        assert!(paths.contains(&"D"));
        assert!(!paths.contains(&"C"), "C should be excluded by edge type filter");
    }

    #[test]
    fn test_search_semantic_with_embedding() {
        use crate::vector_index::VectorIndex;

        // Create a vector index with some test vectors.
        let mut vi = VectorIndex::new(384, 100, 200, 16);

        // Helper to make a deterministic normalized vector.
        let make_vec = |seed: usize| -> Vec<f32> {
            let v: Vec<f32> =
                (0..384).map(|i| ((seed * 7 + i * 13) % 100) as f32 / 100.0).collect();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        };

        let v_rust = make_vec(42);
        let v_python = make_vec(99);
        let v_java = make_vec(200);

        vi.add(&v_rust, "notes/rust.md", Some(0), false, "docs").unwrap();
        vi.add(&v_python, "notes/python.md", Some(0), false, "docs").unwrap();
        vi.add(&v_java, "notes/java.md", Some(0), false, "docs").unwrap();

        // Search with the rust vector — should find rust.md first.
        let results =
            search_semantic_with_embedding(&vi, &v_rust, 3, false, Modality::Both).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "notes/rust.md");
        // Should have vector score > 0 in components.
        assert!(results[0].score_components.as_ref().unwrap().vector > 0.0);

        // BM25 and graph should be 0.
        assert_eq!(results[0].score_components.as_ref().unwrap().bm25, 0.0);
        assert_eq!(results[0].score_components.as_ref().unwrap().graph_boost, 0.0);
    }

    #[test]
    fn test_search_semantic_empty_index() {
        use crate::vector_index::VectorIndex;

        let vi = VectorIndex::new_default(384);
        let query_vec = vec![0.1_f32; 384];

        let results =
            search_semantic_with_embedding(&vi, &query_vec, 10, false, Modality::Both).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_rrf_fuse_merges_lists() {
        // Test the RRF fusion logic directly.
        let list1 = vec![
            SearchResult::new("A", 0.9).with_chunk_index(Some(0)).with_score_components(
                ScoreBreakdown { bm25: 0.0, vector: 0.9, graph_boost: 0.0, graph_hops: None },
            ),
            SearchResult::new("B", 0.7).with_chunk_index(Some(0)).with_score_components(
                ScoreBreakdown { bm25: 0.0, vector: 0.7, graph_boost: 0.0, graph_hops: None },
            ),
        ];
        let list2 = vec![
            SearchResult::new("B", 0.95).with_score_components(ScoreBreakdown {
                bm25: 0.0,
                vector: 0.95,
                graph_boost: 0.0,
                graph_hops: None,
            }),
            SearchResult::new("C", 0.8).with_score_components(ScoreBreakdown {
                bm25: 0.0,
                vector: 0.8,
                graph_boost: 0.0,
                graph_hops: None,
            }),
        ];

        let fused = rrf_fuse(&[&list1, &list2], 5);

        // B appears in both lists, so it should have the highest RRF score.
        assert!(!fused.is_empty());
        assert_eq!(fused[0].path, "B", "B should be top since it appears in both lists");

        // All three docs should appear.
        let paths: Vec<&str> = fused.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"A"));
        assert!(paths.contains(&"B"));
        assert!(paths.contains(&"C"));

        // RRF scores should be in descending order.
        for window in fused.windows(2) {
            assert!(window[0].score >= window[1].score);
        }
    }

    #[test]
    fn test_search_depth_precise_only_chunks() {
        use crate::vector_index::VectorIndex;

        let mut vi = VectorIndex::new(384, 100, 200, 16);

        let make_vec = |seed: usize| -> Vec<f32> {
            let v: Vec<f32> =
                (0..384).map(|i| ((seed * 7 + i * 13) % 100) as f32 / 100.0).collect();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        };

        // Add chunk-level and doc-level vectors for the same document.
        let v_chunk = make_vec(10);
        let v_doc = make_vec(20);

        vi.add(&v_chunk, "notes/a.md", Some(0), false, "docs").unwrap();
        vi.add(&v_doc, "notes/a.md", None, true, "docs").unwrap();

        // Search with precise (chunk only) — should not return doc-level.
        let results =
            search_semantic_with_embedding(&vi, &v_chunk, 10, false, Modality::Both).unwrap();
        // Should find both since doc_level_only=false doesn't exclude chunks
        assert!(!results.is_empty());

        // Search with broad (doc only) — should only return doc-level.
        let results_broad =
            search_semantic_with_embedding(&vi, &v_doc, 10, true, Modality::Both).unwrap();
        for r in &results_broad {
            // All results from doc_level_only=true should not have chunk_index
            assert!(r.chunk_index.is_none(), "broad mode should return doc-level only");
        }
    }

    #[test]
    fn test_decompose_query_multi_concepts() {
        // "How does prompt engineering relate to agent architecture and tool use?"
        let result = decompose_query(
            "How does prompt engineering relate to agent architecture and tool use?",
        );
        assert_eq!(
            result,
            vec![
                "prompt engineering".to_string(),
                "agent architecture".to_string(),
                "tool use?".to_string(),
            ]
        );
    }

    #[test]
    fn test_decompose_query_connect_pattern() {
        // "How do embeddings connect RAG to knowledge graphs?"
        let result = decompose_query("How do embeddings connect RAG to knowledge graphs?");
        assert_eq!(
            result,
            vec!["embeddings".to_string(), "RAG".to_string(), "knowledge graphs?".to_string(),]
        );
    }

    #[test]
    fn test_decompose_query_single_concept() {
        // A simple query with no connecting words should return one concept.
        let result = decompose_query("rust programming language");
        assert_eq!(result, vec!["rust programming language".to_string()]);
    }

    #[test]
    fn test_decompose_query_strips_prefix() {
        // "What is machine learning?" should strip "What is" prefix.
        let result = decompose_query("What is machine learning?");
        assert_eq!(result, vec!["machine learning?".to_string()]);
    }

    #[test]
    fn test_lineage_enrichment_in_search() {
        let index = setup_bm25();
        let mut graph = setup_graph();

        // Add structural edge: notes/async.md supersedes notes/rust.md
        graph.add_edge(
            "notes/async.md",
            "notes/rust.md",
            "supersedes",
            1.0,
            EdgeProvenance::Frontmatter,
            ctxvault_common::config::EdgeClass::Structural,
        );

        let results = search_hybrid(
            &index,
            &graph,
            "systems programming",
            5,
            2,
            None,
            None,
            Modality::Both,
            &no_code(),
        )
        .unwrap();
        assert!(!results.is_empty());

        let rust_result = results.iter().find(|r| r.path == "notes/rust.md").unwrap();
        assert!(rust_result.lineage.is_some(), "SearchResult for rust.md should have lineage");
        let lineage = rust_result.lineage.as_ref().unwrap();
        assert_eq!(lineage.superseded_by, vec!["notes/async.md"]);
    }

    #[test]
    fn test_rrf_fuse_cross_corpus_merges_ranks_and_tags() {
        // Two per-corpus ranked lists, built by hand (no embedder, no manager).
        // Corpus "docs": shared.md is rank 1 (strongest), a.md rank 2.
        // Corpus "code": shared.md is rank 1, b.md rank 2.
        let docs = vec![
            SearchResult::new("shared.md", 9.0).with_snippet(Some("docs shared".into())),
            SearchResult::new("a.md", 8.0),
        ];
        let code = vec![
            SearchResult::new("shared.md", 7.0).with_language("rust").with_chunk_index(Some(3)),
            SearchResult::new("b.md", 6.0),
        ];

        let tagged = vec![("docs".to_string(), docs), ("code".to_string(), code)];
        let fused = rrf_fuse_cross_corpus(&tagged, 10);

        // (b) same path in two corpora stays as two distinct hits.
        let shared_hits: Vec<&SearchResult> =
            fused.iter().filter(|r| r.path == "shared.md").collect();
        assert_eq!(shared_hits.len(), 2, "shared.md must appear once per corpus");

        // (c) every result is tagged with its origin corpus.
        assert!(fused.iter().all(|r| r.corpus.is_some()), "all hits must be corpus-tagged");
        let docs_shared =
            fused.iter().find(|r| r.path == "shared.md" && r.corpus.as_deref() == Some("docs"));
        let code_shared =
            fused.iter().find(|r| r.path == "shared.md" && r.corpus.as_deref() == Some("code"));
        assert!(docs_shared.is_some(), "docs/shared.md must be present and tagged 'docs'");
        assert!(code_shared.is_some(), "code/shared.md must be present and tagged 'code'");

        // Rich fields are preserved from the source result.
        assert_eq!(docs_shared.unwrap().snippet.as_deref(), Some("docs shared"));
        assert_eq!(code_shared.unwrap().language.as_deref(), Some("rust"));
        assert_eq!(code_shared.unwrap().chunk_index, Some(3));

        // (a) RRF ranks: both shared.md hits are rank-1 in their lists → identical RRF
        // score, and each strictly beats the rank-2 hit from the same corpus.
        let a_hit = fused.iter().find(|r| r.path == "a.md").unwrap();
        assert!(
            docs_shared.unwrap().score > a_hit.score,
            "rank-1 shared.md must outrank rank-2 a.md"
        );
        // Top result overall is a shared.md hit.
        assert_eq!(fused[0].path, "shared.md");
    }
}
