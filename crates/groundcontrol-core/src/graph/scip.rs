//! SCIP (Source Code Intelligence Protocol) Protobuf Index Ingestion.
//!
//! Ingests pre-computed SCIP indices (e.g. from `scip-rust`, `scip-typescript`,
//! `scip-python`, `scip-go`, `scip-java`, `scip-clang`) to hydrate the knowledge
//! graph with deterministic compiler-grade symbol definitions and cross-file references
//! at [`ResolutionConfidence::High`].

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use groundcontrol_common::types::{CodeSymbolType, Edge, EdgeProvenance, ResolutionConfidence};
use groundcontrol_common::{Error, Result};
use protobuf::{Enum, Message};
use serde::{Deserialize, Serialize};

use crate::parser::code::manifest::PackageManifest;

/// Statistics reported after ingesting a SCIP index file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipIngestStats {
    /// Total documents/files encountered in the SCIP index.
    pub documents_processed: usize,
    /// Total symbol definitions extracted.
    pub definitions_extracted: usize,
    /// Total calls and references extracted.
    pub calls_extracted: usize,
    /// Total graph edges generated and inserted.
    pub edges_added: usize,
}

/// Extract a bare, comparable leaf identifier from a SCIP moniker string.
///
/// A SCIP symbol (moniker) is a space-separated string of the form
/// `"<scheme> <manager> <package> <version> <descriptors...>"`, e.g.
/// `"scip-rust cargo groundcontrol-core 0.0.22 search()."` or
/// `"scip-rust cargo groundcontrol-core 0.0.22 Engine#"`. The trailing descriptor
/// segment carries the symbol name plus a suffix marker that encodes its kind
/// (`().` for a method/function, `#` for a type, `/` for a namespace, `.` for a
/// term). This strips the scheme/manager/package/version prefix and the trailing
/// descriptor markers, returning the bare identifier (`search`, `Engine`).
///
/// Returns `None` for an empty string, a `local …` moniker, or anything with no
/// recoverable identifier — callers treat `None` as "not comparable" and skip it.
/// This is deliberately language-agnostic (invariant I6): it operates purely on
/// the moniker grammar, not on any single language's naming rules.
pub fn moniker_leaf(symbol: &str) -> Option<String> {
    let symbol = symbol.trim();
    if symbol.is_empty() || symbol.starts_with("local ") {
        return None;
    }

    // The descriptor is the final whitespace-separated segment. For a well-formed
    // moniker the earlier segments are scheme/manager/package/version; a bare
    // identifier with no spaces is treated as its own descriptor.
    let descriptor = symbol.rsplit(' ').next().unwrap_or(symbol);

    // Strip trailing descriptor suffix markers: `().` (method/function),
    // `#` (type), `/` (namespace), `.` (term), and any `()` parameter hint.
    let leaf =
        descriptor.trim_end_matches("().").trim_end_matches("()").trim_end_matches(['#', '/', '.']);

    // A descriptor can be nested (`Engine#search().` -> `Engine#search` after the
    // trim above); take the final `#`/`/`/`.`-separated identifier segment.
    let leaf = leaf.rsplit(['#', '/', '.', '(']).find(|s| !s.is_empty()).unwrap_or(leaf);

    let leaf = leaf.trim();
    if leaf.is_empty() {
        None
    } else {
        Some(leaf.to_string())
    }
}

/// Heuristic: does `name` look like a SCIP moniker (as opposed to a plain
/// qualified name)? SCIP monikers are space-separated and carry a scheme token.
/// Used by the cross-corpus resolver to scan a graph for moniker nodes without a
/// separate moniker store (invariant I5: no new index-time requirement).
pub fn looks_like_moniker(name: &str) -> bool {
    let name = name.trim();
    name.contains(' ') && (name.starts_with("scip-") || name.starts_with("local "))
}

/// Synthesize a deterministic SCIP symbol (moniker) URI in-process.
///
/// Formats standard SCIP URI monikers of the form:
/// `"<scheme> <manager> <package> <version> <descriptors...>"`
/// e.g. `"scip-rust cargo groundcontrol-core 0.2.2 Engine#search()."`
/// or `"scip-go gomod github.com/open-telemetry/... OrderService#PlaceOrder()."`
pub fn synthesize_moniker(
    manifest: Option<&PackageManifest>,
    file_path: &str,
    container: Option<&str>,
    symbol_name: &str,
    symbol_type: CodeSymbolType,
) -> String {
    let (scheme, manager, package, version) = if let Some(m) = manifest {
        (m.scheme.as_str(), m.manager.as_str(), m.package_name.as_str(), m.version.as_str())
    } else {
        let (s, m) = infer_scheme_and_manager(file_path);
        let pkg = Path::new(file_path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("pkg");
        (s, m, pkg, "0.0.0")
    };

    let descriptor = match symbol_type {
        CodeSymbolType::Function | CodeSymbolType::Method => {
            if let Some(c) = container {
                format!("{c}#{symbol_name}().")
            } else {
                format!("{symbol_name}().")
            }
        }
        CodeSymbolType::Class
        | CodeSymbolType::Struct
        | CodeSymbolType::Interface
        | CodeSymbolType::Trait
        | CodeSymbolType::Enum
        | CodeSymbolType::TypeAlias => {
            if let Some(c) = container {
                format!("{c}#{symbol_name}#")
            } else {
                format!("{symbol_name}#")
            }
        }
        CodeSymbolType::Module => {
            if let Some(c) = container {
                format!("{c}#{symbol_name}/")
            } else {
                format!("{symbol_name}/")
            }
        }
        CodeSymbolType::Constant | CodeSymbolType::Route => {
            if let Some(c) = container {
                format!("{c}#{symbol_name}.")
            } else {
                format!("{symbol_name}.")
            }
        }
    };

    format!("{scheme} {manager} {package} {version} {descriptor}")
}

fn infer_scheme_and_manager(file_path: &str) -> (&'static str, &'static str) {
    let ext = Path::new(file_path).extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => ("scip-rust", "cargo"),
        "go" => ("scip-go", "gomod"),
        "ts" | "tsx" | "js" | "jsx" => ("scip-typescript", "npm"),
        "py" => ("scip-python", "pip"),
        "java" | "kt" | "scala" => ("scip-java", "maven"),
        "cs" => ("scip-dotnet", "nuget"),
        "c" | "cpp" | "cc" | "h" | "hpp" => ("scip-clang", "cmake"),
        "rb" => ("scip-ruby", "gem"),
        "php" => ("scip-php", "composer"),
        _ => ("scip-generic", "local"),
    }
}

/// Ingester for SCIP protobuf index files.
pub struct ScipIngester;

impl ScipIngester {
    /// Ingest a SCIP binary protobuf file from disk.
    pub fn extract_edges_from_file(scip_path: &Path) -> Result<(Vec<Edge>, ScipIngestStats)> {
        Self::extract_edges_from_file_with_prefix(scip_path, None)
    }

    /// Ingest a SCIP binary protobuf file from disk with an optional repository-relative base prefix.
    pub fn extract_edges_from_file_with_prefix(
        scip_path: &Path,
        base_prefix: Option<&str>,
    ) -> Result<(Vec<Edge>, ScipIngestStats)> {
        let file = File::open(scip_path).map_err(Error::Io)?;
        let mut reader = BufReader::new(file);
        let index = scip::types::Index::parse_from_reader(&mut reader)
            .map_err(|e| Error::Graph(format!("failed to parse SCIP protobuf index: {}", e)))?;

        Self::extract_edges_from_index_with_prefix(&index, base_prefix)
    }

    /// Extract graph edges and statistics from an in-memory SCIP index.
    pub fn extract_edges_from_index(
        index: &scip::types::Index,
    ) -> Result<(Vec<Edge>, ScipIngestStats)> {
        Self::extract_edges_from_index_with_prefix(index, None)
    }

    /// Extract graph edges and statistics from an in-memory SCIP index with an optional repository-relative base prefix.
    pub fn extract_edges_from_index_with_prefix(
        index: &scip::types::Index,
        base_prefix: Option<&str>,
    ) -> Result<(Vec<Edge>, ScipIngestStats)> {
        let mut edges = Vec::new();
        let mut stats = ScipIngestStats::default();

        let def_role = scip::types::SymbolRole::Definition.value();

        for doc in &index.documents {
            stats.documents_processed += 1;
            let raw_path = doc.relative_path.replace('\\', "/");
            let doc_path = match base_prefix {
                Some(prefix) if !prefix.is_empty() => {
                    let clean_prefix = prefix.trim_matches(['/', '\\']).replace('\\', "/");
                    if raw_path.starts_with(&clean_prefix) {
                        raw_path
                    } else {
                        format!("{clean_prefix}/{raw_path}")
                    }
                }
                _ => raw_path,
            };

            // Track definitions with line ranges to associate calls
            // (start_line, end_line, symbol)
            let mut definitions: Vec<(i32, i32, String)> = Vec::new();
            let mut references: Vec<(i32, String)> = Vec::new();

            for occ in &doc.occurrences {
                let symbol = occ.symbol.clone();
                if symbol.is_empty() || symbol.starts_with("local ") {
                    continue;
                }

                let range = &occ.range;
                let start_line = range.first().copied().unwrap_or(0);
                let end_line = if range.len() >= 3 { range[2] } else { start_line };

                let is_definition = (occ.symbol_roles & def_role) != 0;
                if is_definition {
                    stats.definitions_extracted += 1;
                    edges.push(Edge {
                        source: doc_path.clone(),
                        target: symbol.clone(),
                        edge_type: "defines".to_string(),
                        weight: 1.0,
                        provenance: EdgeProvenance::CodeDefines,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::High),
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });
                    definitions.push((start_line, end_line, symbol));
                } else {
                    references.push((start_line, symbol));
                }
            }

            // For references inside a definition's enclosing scope, create a calls edge
            for (ref_line, target_symbol) in references {
                let caller = definitions
                    .iter()
                    .filter(|(start, end, _)| *start <= ref_line && ref_line <= *end)
                    .min_by_key(|(start, end, _)| end - start);

                let source = if let Some((_, _, caller_sym)) = caller {
                    if caller_sym == &target_symbol {
                        continue;
                    }
                    caller_sym.clone()
                } else {
                    doc_path.clone()
                };

                stats.calls_extracted += 1;
                edges.push(Edge {
                    source,
                    target: target_symbol,
                    edge_type: "calls".to_string(),
                    weight: 0.8,
                    provenance: EdgeProvenance::CodeCalls,
                    target_corpus: None,
                    confidence: Some(ResolutionConfidence::High),
                    target_path: None,
                    target_symbol: None,
                    target_kind: None,
                });
            }
        }

        stats.edges_added = edges.len();
        Ok((edges, stats))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scip::types::{Document, Index, Occurrence, SymbolRole};

    #[test]
    fn test_moniker_leaf_extracts_bare_identifier() {
        // Function/method moniker: trailing `().`.
        assert_eq!(
            moniker_leaf("scip-rust cargo groundcontrol-core 0.0.22 search()."),
            Some("search".to_string())
        );
        // Type moniker: trailing `#`.
        assert_eq!(
            moniker_leaf("scip-rust cargo groundcontrol-core 0.0.22 Engine#"),
            Some("Engine".to_string())
        );
        // Nested descriptor: `Engine#search().` -> leaf `search`.
        assert_eq!(
            moniker_leaf("scip-rust cargo groundcontrol-core 0.0.22 Engine#search()."),
            Some("search".to_string())
        );
        // Namespace/term markers.
        assert_eq!(
            moniker_leaf("scip-typescript npm pkg 1.0.0 search."),
            Some("search".to_string())
        );
        // Absence / non-comparable inputs return None (I5 fall-through).
        assert_eq!(moniker_leaf(""), None);
        assert_eq!(moniker_leaf("local 0"), None);
    }

    #[test]
    fn test_looks_like_moniker() {
        assert!(looks_like_moniker("scip-rust cargo groundcontrol-core 0.0.22 search()."));
        assert!(!looks_like_moniker("search"));
        assert!(!looks_like_moniker("crate::search::Engine"));
    }

    #[test]
    fn test_scip_index_ingestion() {
        let mut index = Index::new();

        let mut doc = Document::new();
        doc.relative_path = "src/search.rs".to_string();

        // 1. Definition occurrence: search function
        let mut occ_def = Occurrence::new();
        occ_def.symbol = "scip-rust cargo groundcontrol-core 0.0.22 search().".to_string();
        occ_def.symbol_roles = SymbolRole::Definition.value();
        occ_def.range = vec![10, 0, 20, 1]; // lines 10..20
        doc.occurrences.push(occ_def);

        // 2. Reference occurrence inside search: calls rrf_fuse
        let mut occ_ref = Occurrence::new();
        occ_ref.symbol = "scip-rust cargo groundcontrol-core 0.0.22 rrf_fuse().".to_string();
        occ_ref.symbol_roles = 0;
        occ_ref.range = vec![15, 4, 15, 12]; // line 15 inside 10..20
        doc.occurrences.push(occ_ref);

        index.documents.push(doc);

        let (edges, stats) = ScipIngester::extract_edges_from_index(&index).unwrap();

        assert_eq!(stats.documents_processed, 1);
        assert_eq!(stats.definitions_extracted, 1);
        assert_eq!(stats.calls_extracted, 1);
        assert_eq!(stats.edges_added, 2);

        // Check defines edge
        let def_edge = edges.iter().find(|e| e.edge_type == "defines").unwrap();
        assert_eq!(def_edge.source, "src/search.rs");
        assert_eq!(def_edge.target, "scip-rust cargo groundcontrol-core 0.0.22 search().");
        assert_eq!(def_edge.confidence, Some(ResolutionConfidence::High));

        // Check calls edge
        let call_edge = edges.iter().find(|e| e.edge_type == "calls").unwrap();
        assert_eq!(call_edge.source, "scip-rust cargo groundcontrol-core 0.0.22 search().");
        assert_eq!(call_edge.target, "scip-rust cargo groundcontrol-core 0.0.22 rrf_fuse().");
        assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
    }

    #[test]
    fn test_synthesize_moniker_matches_spec() {
        let manifest = PackageManifest::new(
            "scip-go",
            "gomod",
            "github.com/open-telemetry/opentelemetry-demo/src/checkoutservice",
            "1.2.0",
            "/repo/src/checkoutservice",
        );

        let m1 = synthesize_moniker(
            Some(&manifest),
            "src/checkoutservice/main.go",
            Some("OrderService"),
            "PlaceOrder",
            CodeSymbolType::Method,
        );
        assert_eq!(
            m1,
            "scip-go gomod github.com/open-telemetry/opentelemetry-demo/src/checkoutservice 1.2.0 OrderService#PlaceOrder()."
        );
        assert!(looks_like_moniker(&m1));
        assert_eq!(moniker_leaf(&m1), Some("PlaceOrder".to_string()));

        let m2 = synthesize_moniker(
            Some(&manifest),
            "src/checkoutservice/main.go",
            None,
            "OrderService",
            CodeSymbolType::Struct,
        );
        assert_eq!(
            m2,
            "scip-go gomod github.com/open-telemetry/opentelemetry-demo/src/checkoutservice 1.2.0 OrderService#"
        );
        assert!(looks_like_moniker(&m2));
        assert_eq!(moniker_leaf(&m2), Some("OrderService".to_string()));

        // Fallback without manifest
        let m3 = synthesize_moniker(
            None,
            "crates/search/engine.rs",
            Some("SearchEngine"),
            "query",
            CodeSymbolType::Method,
        );
        assert!(m3.starts_with("scip-rust cargo search 0.0.0 SearchEngine#query()."));
        assert!(looks_like_moniker(&m3));
        assert_eq!(moniker_leaf(&m3), Some("query".to_string()));
    }

    #[test]
    fn test_scip_index_ingestion_with_prefix() {
        let mut index = Index::new();
        let mut doc = Document::new();
        doc.relative_path = "handler.go".to_string();

        let mut occ_def = Occurrence::new();
        occ_def.symbol = "scip-go gomod github.com/otel/checkout 1.0.0 HandleOrder().".to_string();
        occ_def.symbol_roles = SymbolRole::Definition.value();
        occ_def.range = vec![5, 0, 15, 1];
        doc.occurrences.push(occ_def);

        index.documents.push(doc);

        let (edges, stats) =
            ScipIngester::extract_edges_from_index_with_prefix(&index, Some("src/checkoutservice"))
                .unwrap();

        assert_eq!(stats.documents_processed, 1);
        assert_eq!(stats.definitions_extracted, 1);
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].source, "src/checkoutservice/handler.go");
        assert_eq!(edges[0].target, "scip-go gomod github.com/otel/checkout 1.0.0 HandleOrder().");
    }
}
