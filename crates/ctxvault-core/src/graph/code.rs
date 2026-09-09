//! Polyglot Code Graph Extractor & Lightweight Symbol/Import Resolver.
//!
//! Extracts structural AST relationships (`defines`, `imports`, `calls`, `implements_trait`)
//! across polyglot source code files and resolves cross-file call sites using SQLite
//! symbol catalogs and Petgraph.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ctxvault_common::types::{CodeSymbol, Edge, EdgeProvenance, ResolutionConfidence};
use tree_sitter::{Node, Parser};

use crate::graph::hybrid_lsp::{clean_type_name, TypeEnvironment};
use crate::parser::code::languages::{detect_language, SupportedLanguage};
use crate::parser::code::spec::get_language_spec;

/// Extracted structural code relationship.
#[derive(Debug, Clone)]
pub struct ExtractedCodeEdge {
    /// Source node path (file path or symbol scope path).
    pub source: String,
    /// Target node path (file path or symbol scope path).
    pub target: String,
    /// Edge relationship type (e.g. "defines", "imports", "calls", "implements_trait").
    pub edge_type: String,
    /// Edge weight (0.0 - 1.0).
    pub weight: f32,
    /// Edge provenance.
    pub provenance: EdgeProvenance,
}

/// Polyglot code graph extractor.
pub struct CodeGraphExtractor;

impl CodeGraphExtractor {
    /// Build a symbol lookup index from a slice of code symbols.
    pub fn build_symbol_index<'a>(
        symbols: &'a [CodeSymbol],
    ) -> HashMap<String, Vec<&'a CodeSymbol>> {
        let mut symbol_index: HashMap<String, Vec<&CodeSymbol>> =
            HashMap::with_capacity(symbols.len());
        for sym in symbols {
            symbol_index.entry(sym.name.clone()).or_default().push(sym);
        }
        symbol_index
    }

    /// Extract all structural edges (defines, imports, calls, implements) for a single code file
    /// using a pre-computed symbol index.
    pub fn extract_edges_for_file_with_index(
        file_path: &Path,
        content: &str,
        file_symbols: &[CodeSymbol],
        symbol_index: &HashMap<String, Vec<&CodeSymbol>>,
    ) -> Vec<Edge> {
        let mut edges = Vec::new();
        let file_path_str = file_path.to_string_lossy().replace('\\', "/");

        // 1. "defines" edges: File -> Symbol
        for sym in file_symbols {
            edges.push(Edge {
                source: file_path_str.clone(),
                target: sym.scope_path.clone(),
                edge_type: "defines".to_string(),
                weight: 1.0,
                provenance: EdgeProvenance::CodeDefines,
                target_corpus: None,
                confidence: Some(ResolutionConfidence::High),
            });
        }

        // 2. Parse AST for imports and call sites
        let Some(lang) = detect_language(file_path) else {
            return edges;
        };

        let mut parser = Parser::new();
        if parser.set_language(&lang.tree_sitter_language()).is_err() {
            return edges;
        }

        let Some(tree) = parser.parse(content, None) else {
            return edges;
        };

        let mut visitor =
            CallAndImportVisitor::new(file_path_str, content, lang, file_symbols, symbol_index);
        visitor.visit(tree.root_node());

        edges.extend(visitor.edges);
        edges
    }

    /// Extract all structural edges (defines, imports, calls, implements) for a single code file.
    pub fn extract_edges_for_file(
        file_path: &Path,
        content: &str,
        file_symbols: &[CodeSymbol],
        all_symbols: &[CodeSymbol],
    ) -> Vec<Edge> {
        let symbol_index = Self::build_symbol_index(all_symbols);
        Self::extract_edges_for_file_with_index(file_path, content, file_symbols, &symbol_index)
    }
}

struct CallAndImportVisitor<'a> {
    file_path: String,
    content: &'a str,
    language: SupportedLanguage,
    file_symbols: &'a [CodeSymbol],
    symbol_index: &'a HashMap<String, Vec<&'a CodeSymbol>>,
    current_caller: Option<String>,
    edges: Vec<Edge>,
    visited_calls: HashSet<(String, String)>,
    type_env: TypeEnvironment,
}

impl<'a> CallAndImportVisitor<'a> {
    fn new(
        file_path: String,
        content: &'a str,
        language: SupportedLanguage,
        file_symbols: &'a [CodeSymbol],
        symbol_index: &'a HashMap<String, Vec<&'a CodeSymbol>>,
    ) -> Self {
        Self {
            file_path,
            content,
            language,
            file_symbols,
            symbol_index,
            current_caller: None,
            edges: Vec::new(),
            visited_calls: HashSet::new(),
            type_env: TypeEnvironment::new(language),
        }
    }

    fn node_text(&self, node: Node) -> &str {
        &self.content[node.start_byte()..node.end_byte()]
    }

    fn visit(&mut self, node: Node) {
        let kind = node.kind();
        let spec = get_language_spec(self.language);

        // Track container scope (class, struct, trait, interface, or Rust impl)
        let is_rust_impl = self.language == SupportedLanguage::Rust && kind == "impl_item";
        let is_container = is_rust_impl
            || spec.class_node_kinds.contains(&kind)
            || spec.struct_node_kinds.contains(&kind)
            || spec.trait_node_kinds.contains(&kind)
            || spec.interface_node_kinds.contains(&kind);

        if is_container {
            let container_name = if is_rust_impl {
                node.child_by_field_name("type").map(|t| clean_type_name(self.node_text(t)))
            } else {
                node.child_by_field_name("name").map(|n| clean_type_name(self.node_text(n)))
            };

            self.type_env.push_scope(container_name);

            if is_rust_impl {
                self.extract_implements(node);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit(child);
            }

            self.type_env.pop_scope();
            return;
        }

        // Track caller function/method scope
        if spec.is_callable(kind) {
            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let matching_sym = self
                .file_symbols
                .iter()
                .filter(|s| s.start_line <= start_line && s.end_line >= end_line)
                .min_by_key(|s| s.end_line - s.start_line);

            let prev_caller = self.current_caller.take();
            if let Some(sym) = matching_sym {
                self.current_caller = Some(sym.scope_path.clone());
            } else {
                self.current_caller = prev_caller.clone();
            }

            self.type_env.push_scope(None);

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.type_env.inspect_node(child, self.content);
                self.visit(child);
            }

            self.type_env.pop_scope();
            self.current_caller = prev_caller;
            return;
        }

        // Inside a body: inspect statements/declarations for variable bindings
        self.type_env.inspect_node(node, self.content);

        // Extract imports
        if spec.is_import(kind) {
            self.extract_import(node);
        }

        // Extract call expressions
        if spec.is_call(kind) {
            self.extract_call(node);
        }

        // Extract trait implementations
        self.extract_implements(node);

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child);
        }
    }

    fn extract_import(&mut self, node: Node) {
        let kind = node.kind();
        match self.language {
            SupportedLanguage::Rust => {
                if kind == "use_declaration" {
                    let text = self.node_text(node).trim().trim_end_matches(';').trim();
                    if let Some(target) = text.strip_prefix("use ") {
                        let target_str = target.trim().to_string();
                        let sym = target_str.rsplit("::").next().unwrap_or(&target_str).to_string();
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: target_str.clone(),
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                            target_corpus: None,
                            confidence: Some(ResolutionConfidence::Speculative),
                        });
                        self.type_env.register_import(sym, target_str);
                    }
                }
            }
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                if kind == "import_statement" {
                    if let Some(source_node) = node.child_by_field_name("source") {
                        let raw = self
                            .node_text(source_node)
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: raw,
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                            target_corpus: None,
                            confidence: Some(ResolutionConfidence::Speculative),
                        });
                    }
                }
            }
            SupportedLanguage::Python => {
                if kind == "import_statement" || kind == "import_from_statement" {
                    let text = self.node_text(node).trim().to_string();
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: text,
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                    });
                }
            }
            SupportedLanguage::Go => {
                if kind == "import_spec" {
                    let path = self.node_text(node).trim().trim_matches('"').to_string();
                    let pkg = path.rsplit('/').next().unwrap_or(&path).to_string();
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: path.clone(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                    });
                    self.type_env.register_import(pkg, path);
                }
            }
            _ => {
                let text = self.node_text(node).trim().trim_end_matches(';').trim();
                let clean = text
                    .strip_prefix("import ")
                    .or_else(|| text.strip_prefix("#include "))
                    .or_else(|| text.strip_prefix("include "))
                    .or_else(|| text.strip_prefix("using "))
                    .unwrap_or(text)
                    .trim()
                    .trim_matches('"')
                    .trim_matches('<')
                    .trim_matches('>');
                if !clean.is_empty() && clean.len() < 200 {
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: clean.to_string(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                    });
                }
            }
        }
    }

    fn extract_call(&mut self, node: Node) {
        let Some(ref caller) = self.current_caller else {
            return;
        };

        let Some((receiver, callee)) = self.extract_call_parts(node) else {
            return;
        };

        if let Some((target_sym, confidence)) = self.resolve_callee(receiver.as_deref(), &callee) {
            let key = (caller.clone(), target_sym.scope_path.clone());
            if !self.visited_calls.contains(&key) && caller != &target_sym.scope_path {
                self.visited_calls.insert(key);
                self.edges.push(Edge {
                    source: caller.clone(),
                    target: target_sym.scope_path.clone(),
                    edge_type: "calls".to_string(),
                    weight: 0.8,
                    provenance: EdgeProvenance::CodeCalls,
                    target_corpus: None,
                    confidence: Some(confidence),
                });
            }
        } else {
            let target = match receiver.as_deref() {
                Some(rec) if !rec.is_empty() => format!("{}.{}", rec, callee),
                _ => callee.clone(),
            };
            let key = (caller.clone(), target.clone());
            if !self.visited_calls.contains(&key) && caller != &target {
                self.visited_calls.insert(key);
                self.edges.push(Edge {
                    source: caller.clone(),
                    target,
                    edge_type: "calls".to_string(),
                    weight: 0.5,
                    provenance: EdgeProvenance::CodeCalls,
                    target_corpus: None,
                    confidence: Some(ResolutionConfidence::Speculative),
                });
            }
        }
    }

    fn extract_call_parts(&self, node: Node) -> Option<(Option<String>, String)> {
        let kind = node.kind();
        if kind == "method_call_expression" {
            let method = node.child_by_field_name("name")?;
            let receiver =
                node.child_by_field_name("receiver").map(|r| self.node_text(r).trim().to_string());
            return Some((receiver, self.node_text(method).trim().to_string()));
        }

        if kind == "method_invocation" {
            let method = node.child_by_field_name("name")?;
            let receiver =
                node.child_by_field_name("object").map(|r| self.node_text(r).trim().to_string());
            return Some((receiver, self.node_text(method).trim().to_string()));
        }

        if kind == "call_expression" || kind == "call" || kind == "function_call" {
            let func = node.child_by_field_name("function").or_else(|| node.child(0))?;
            let func_kind = func.kind();

            if func_kind == "field_expression" {
                let field = func.child_by_field_name("field")?;
                let receiver = func
                    .child_by_field_name("value")
                    .or_else(|| func.child_by_field_name("argument"))
                    .map(|a| self.node_text(a).trim().to_string());
                return Some((receiver, self.node_text(field).trim().to_string()));
            }

            if func_kind == "scoped_identifier" {
                let name = func.child_by_field_name("name")?;
                let path =
                    func.child_by_field_name("path").map(|p| self.node_text(p).trim().to_string());
                return Some((path, self.node_text(name).trim().to_string()));
            }

            if func_kind == "member_expression" {
                let prop = func.child_by_field_name("property")?;
                let obj = func
                    .child_by_field_name("object")
                    .map(|o| self.node_text(o).trim().to_string());
                return Some((obj, self.node_text(prop).trim().to_string()));
            }

            if func_kind == "attribute" {
                let attr = func.child_by_field_name("attribute")?;
                let val =
                    func.child_by_field_name("value").map(|v| self.node_text(v).trim().to_string());
                return Some((val, self.node_text(attr).trim().to_string()));
            }

            if func_kind == "selector_expression" {
                let field = func.child_by_field_name("field")?;
                let operand = func
                    .child_by_field_name("operand")
                    .map(|o| self.node_text(o).trim().to_string());
                return Some((operand, self.node_text(field).trim().to_string()));
            }

            if func_kind == "identifier" || func_kind == "property_identifier" {
                return Some((None, self.node_text(func).trim().to_string()));
            }

            let full = self.node_text(func).trim();
            if let Some((rec, method)) = full.rsplit_once('.') {
                return Some((Some(rec.trim().to_string()), method.trim().to_string()));
            }
            if let Some((rec, method)) = full.rsplit_once("::") {
                return Some((Some(rec.trim().to_string()), method.trim().to_string()));
            }
            if let Some((rec, method)) = full.rsplit_once("->") {
                return Some((Some(rec.trim().to_string()), method.trim().to_string()));
            }
            if !full.is_empty() {
                return Some((None, full.to_string()));
            }
        }

        if kind == "invocation_expression" {
            if let Some(expr) = node.child_by_field_name("expression") {
                if expr.kind() == "member_access_expression" {
                    let name = expr.child_by_field_name("name")?;
                    let expr_node = expr
                        .child_by_field_name("expression")
                        .map(|e| self.node_text(e).trim().to_string());
                    return Some((expr_node, self.node_text(name).trim().to_string()));
                }
            }
        }

        None
    }

    /// Resolve a callee name to a symbol, returning the resolution confidence band.
    ///
    /// When a receiver is present and resolved by Hybrid LSP type tracking, matching
    /// container methods are resolved with [`ResolutionConfidence::High`].
    fn resolve_callee(
        &self,
        receiver: Option<&str>,
        callee_name: &str,
    ) -> Option<(&'a CodeSymbol, ResolutionConfidence)> {
        let clean_name = callee_name.rsplit("::").next().unwrap_or(callee_name);
        let clean_name = clean_name.rsplit('.').next().unwrap_or(clean_name);

        // 0. Hybrid LSP: If receiver is present, attempt type-guided disambiguation
        if let Some(rec) = receiver {
            let rec_clean = rec.trim();
            let resolved_type = self.type_env.resolve_variable_type(rec_clean).or_else(|| {
                if rec_clean.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                    Some(rec_clean.rsplit("::").next().unwrap_or(rec_clean).to_string())
                } else {
                    None
                }
            });

            if let Some(ref type_name) = resolved_type {
                // A. Check within current file symbols
                if let Some(local_match) = self.file_symbols.iter().find(|s| {
                    s.name == clean_name
                        && (s.scope_path.contains(type_name.as_str())
                            || s.scope_path == format!("{type_name} > {clean_name}"))
                }) {
                    return Some((local_match, ResolutionConfidence::High));
                }

                // B. Check workspace symbol catalog
                if let Some(candidates) = self.symbol_index.get(clean_name) {
                    let type_matches: Vec<&&CodeSymbol> = candidates
                        .iter()
                        .filter(|s| {
                            s.scope_path.contains(type_name.as_str())
                                || s.scope_path == format!("{type_name} > {clean_name}")
                        })
                        .collect();

                    if type_matches.len() == 1 {
                        return Some((type_matches[0], ResolutionConfidence::High));
                    } else if !type_matches.is_empty() {
                        let file_dir =
                            Path::new(&self.file_path).parent().unwrap_or_else(|| Path::new(""));
                        if let Some(dir_match) = type_matches.iter().find(|c| {
                            Path::new(&c.file_path).parent().unwrap_or_else(|| Path::new(""))
                                == file_dir
                        }) {
                            return Some((dir_match, ResolutionConfidence::High));
                        }
                        return Some((type_matches[0], ResolutionConfidence::High));
                    }
                }
            }
        }

        // 1. Search within the current file first (fastest and highest confidence)
        if let Some(local_match) = self.file_symbols.iter().find(|s| s.name == clean_name) {
            return Some((local_match, ResolutionConfidence::High));
        }

        // 2. Search in workspace symbols catalog
        if let Some(candidates) = self.symbol_index.get(clean_name) {
            if candidates.len() == 1 {
                return Some((candidates[0], ResolutionConfidence::High));
            }
            // 3. If multiple candidates, prioritize same directory / crate
            let file_dir = Path::new(&self.file_path).parent().unwrap_or_else(|| Path::new(""));
            if let Some(dir_match) = candidates.iter().find(|c| {
                Path::new(&c.file_path).parent().unwrap_or_else(|| Path::new("")) == file_dir
            }) {
                return Some((dir_match, ResolutionConfidence::Medium));
            }
            // 4. Fall back to the first candidate with no disambiguating signal.
            return candidates.first().map(|c| (*c, ResolutionConfidence::Speculative));
        }

        None
    }

    fn extract_implements(&mut self, node: Node) {
        if self.language == SupportedLanguage::Rust && node.kind() == "impl_item" {
            if let Some(trait_node) = node.child_by_field_name("trait") {
                let trait_name = self.node_text(trait_node).trim().to_string();
                if let Some(type_node) = node.child_by_field_name("type") {
                    let type_name = self.node_text(type_node).trim().to_string();
                    self.edges.push(Edge {
                        source: type_name,
                        target: trait_name,
                        edge_type: "implements_trait".to_string(),
                        weight: 0.9,
                        provenance: EdgeProvenance::CodeImplementsTrait,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::High),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::code::chunker::CodeChunker;
    use ctxvault_common::config::ChunkingConfig;

    #[test]
    fn test_code_graph_extraction_calls_and_defines() {
        let code_a = r#"
pub struct SearchEngine;

impl SearchEngine {
    pub fn search(&self, q: &str) -> Vec<String> {
        let results = rrf_fuse(q);
        results
    }
}

pub fn rrf_fuse(q: &str) -> Vec<String> {
    vec![q.to_string()]
}
"#;
        let config = ChunkingConfig::default();
        let parse_res =
            CodeChunker::parse_and_chunk(Path::new("src/search.rs"), code_a, &config).unwrap();
        let edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/search.rs"),
            code_a,
            &parse_res.symbols,
            &parse_res.symbols,
        );

        // Check defines edges
        assert!(edges.iter().any(|e| e.edge_type == "defines" && e.target == "SearchEngine"));
        assert!(edges
            .iter()
            .any(|e| e.edge_type == "defines" && e.target == "SearchEngine > search"));
        assert!(edges.iter().any(|e| e.edge_type == "defines" && e.target == "rrf_fuse"));

        // Check calls edges: SearchEngine > search calls rrf_fuse
        assert!(edges.iter().any(|e| e.edge_type == "calls"
            && e.source == "SearchEngine > search"
            && e.target == "rrf_fuse"));
    }

    #[test]
    fn test_call_edge_confidence_unique_is_high() {
        // rrf_fuse resolves uniquely within the current file -> High confidence.
        let code = r#"
pub fn search(q: &str) -> Vec<String> {
    rrf_fuse(q)
}

pub fn rrf_fuse(q: &str) -> Vec<String> {
    vec![q.to_string()]
}
"#;
        let config = ChunkingConfig::default();
        let parse_res =
            CodeChunker::parse_and_chunk(Path::new("src/search.rs"), code, &config).unwrap();
        let edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/search.rs"),
            code,
            &parse_res.symbols,
            &parse_res.symbols,
        );

        let call_edge = edges
            .iter()
            .find(|e| e.edge_type == "calls" && e.target == "rrf_fuse")
            .expect("expected a call edge to rrf_fuse");
        assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));

        // defines edges are exact -> High.
        let define_edge =
            edges.iter().find(|e| e.edge_type == "defines").expect("expected a defines edge");
        assert_eq!(define_edge.confidence, Some(ResolutionConfidence::High));
    }

    #[test]
    fn test_call_edge_confidence_ambiguous_is_medium_or_speculative() {
        // The caller file has no local `helper`; two workspace candidates named
        // `helper` exist in different files. One shares the caller's directory,
        // so the same-directory heuristic (case 3) applies -> Medium.
        let caller_code = r#"
pub fn run() {
    helper();
}
"#;
        let config = ChunkingConfig::default();
        let caller_res =
            CodeChunker::parse_and_chunk(Path::new("src/a/caller.rs"), caller_code, &config)
                .unwrap();

        // Two distinct `helper` symbols in different files.
        let same_dir = r#"pub fn helper() {}"#;
        let other_dir = r#"pub fn helper() {}"#;
        let same_dir_res =
            CodeChunker::parse_and_chunk(Path::new("src/a/other.rs"), same_dir, &config).unwrap();
        let other_dir_res =
            CodeChunker::parse_and_chunk(Path::new("src/b/other.rs"), other_dir, &config).unwrap();

        let mut all_symbols = caller_res.symbols.clone();
        all_symbols.extend(same_dir_res.symbols.clone());
        all_symbols.extend(other_dir_res.symbols.clone());

        let edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/a/caller.rs"),
            caller_code,
            &caller_res.symbols,
            &all_symbols,
        );

        let call_edge = edges
            .iter()
            .find(|e| e.edge_type == "calls" && e.target == "helper")
            .expect("expected a call edge to helper");
        assert_eq!(
            call_edge.confidence,
            Some(ResolutionConfidence::Medium),
            "same-directory disambiguation should yield Medium confidence"
        );
        assert_ne!(call_edge.confidence, Some(ResolutionConfidence::High));
    }

    #[test]
    fn test_hybrid_lsp_receiver_method_disambiguation_rust() {
        let caller_code = r#"
pub struct QueryService;

impl QueryService {
    pub fn execute(&self) {
        let client = SearchClient::new();
        client.query("rust");
    }
}
"#;
        let search_client_code = r#"
pub struct SearchClient;

impl SearchClient {
    pub fn new() -> Self { SearchClient }
    pub fn query(&self, q: &str) -> Vec<String> { vec![] }
}
"#;
        let db_client_code = r#"
pub struct DatabaseClient;

impl DatabaseClient {
    pub fn query(&self, sql: &str) -> Vec<String> { vec![] }
}
"#;
        let config = ChunkingConfig::default();
        let caller_res =
            CodeChunker::parse_and_chunk(Path::new("src/service.rs"), caller_code, &config)
                .unwrap();
        let search_res =
            CodeChunker::parse_and_chunk(Path::new("src/search.rs"), search_client_code, &config)
                .unwrap();
        let db_res =
            CodeChunker::parse_and_chunk(Path::new("src/db.rs"), db_client_code, &config).unwrap();

        let mut all_symbols = caller_res.symbols.clone();
        all_symbols.extend(search_res.symbols.clone());
        all_symbols.extend(db_res.symbols.clone());

        let edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/service.rs"),
            caller_code,
            &caller_res.symbols,
            &all_symbols,
        );

        let call_edge = edges
            .iter()
            .find(|e| e.edge_type == "calls" && e.target == "SearchClient > query")
            .expect("expected a call edge to SearchClient > query");
        assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
    }

    #[test]
    fn test_hybrid_lsp_receiver_method_disambiguation_typescript() {
        let ts_caller = r#"
export class Controller {
    handleRequest() {
        const client = new ApiClient();
        client.fetchData();
    }
}
"#;
        let ts_target = r#"
export class ApiClient {
    fetchData() {
        return "data";
    }
}
"#;
        let config = ChunkingConfig::default();
        let caller_res =
            CodeChunker::parse_and_chunk(Path::new("src/controller.ts"), ts_caller, &config)
                .unwrap();
        let target_res =
            CodeChunker::parse_and_chunk(Path::new("src/api.ts"), ts_target, &config).unwrap();

        let mut all_symbols = caller_res.symbols.clone();
        all_symbols.extend(target_res.symbols.clone());

        let edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/controller.ts"),
            ts_caller,
            &caller_res.symbols,
            &all_symbols,
        );

        let call_edge = edges
            .iter()
            .find(|e| e.edge_type == "calls" && e.target == "ApiClient > fetchData")
            .expect("expected a call edge to ApiClient > fetchData");
        assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
    }
}
