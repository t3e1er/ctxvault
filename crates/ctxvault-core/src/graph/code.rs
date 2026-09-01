//! Polyglot Code Graph Extractor & Lightweight Symbol/Import Resolver.
//!
//! Extracts structural AST relationships (`defines`, `imports`, `calls`, `implements_trait`)
//! across polyglot source code files and resolves cross-file call sites using SQLite
//! symbol catalogs and Petgraph.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ctxvault_common::types::{CodeSymbol, Edge, EdgeProvenance};
use tree_sitter::{Node, Parser};

use crate::parser::code::languages::{detect_language, SupportedLanguage};

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
    /// Extract all structural edges (defines, imports, calls, implements) for a single code file.
    pub fn extract_edges_for_file(
        file_path: &Path,
        content: &str,
        file_symbols: &[CodeSymbol],
        all_symbols: &[CodeSymbol],
    ) -> Vec<Edge> {
        let mut edges = Vec::new();
        let file_path_str = file_path.to_string_lossy().to_string();

        // 1. "defines" edges: File -> Symbol
        for sym in file_symbols {
            edges.push(Edge {
                source: file_path_str.clone(),
                target: sym.scope_path.clone(),
                edge_type: "defines".to_string(),
                weight: 1.0,
                provenance: EdgeProvenance::CodeDefines,
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

        // Symbol index for fast resolution: name -> Vec<CodeSymbol>
        let mut symbol_index: HashMap<String, Vec<&CodeSymbol>> = HashMap::new();
        for sym in all_symbols {
            symbol_index.entry(sym.name.clone()).or_default().push(sym);
        }

        let mut visitor =
            CallAndImportVisitor::new(file_path_str, content, lang, file_symbols, &symbol_index);
        visitor.visit(tree.root_node());

        edges.extend(visitor.edges);
        edges
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
        }
    }

    fn node_text(&self, node: Node) -> &str {
        &self.content[node.start_byte()..node.end_byte()]
    }

    fn visit(&mut self, node: Node) {
        let kind = node.kind();

        // Track caller function/method scope
        let is_callable = match self.language {
            SupportedLanguage::Rust => kind == "function_item",
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                kind == "function_declaration" || kind == "method_definition" || kind == "function"
            }
            SupportedLanguage::Python => kind == "function_definition",
            SupportedLanguage::Go => kind == "function_declaration" || kind == "method_declaration",
            SupportedLanguage::C
            | SupportedLanguage::Cpp
            | SupportedLanguage::Java
            | SupportedLanguage::CSharp
            | SupportedLanguage::Php
            | SupportedLanguage::Swift
            | SupportedLanguage::Bash => {
                kind == "function_definition"
                    || kind == "method_declaration"
                    || kind == "function_declaration"
                    || kind == "init_declaration"
            }
            SupportedLanguage::Ruby => kind == "method" || kind == "singleton_method",
            SupportedLanguage::Elixir => kind == "call",
            SupportedLanguage::Lua => kind == "function_declaration" || kind == "local_function",
        };

        if is_callable {
            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;
            let matching_sym = self
                .file_symbols
                .iter()
                .filter(|s| s.start_line <= start_line && s.end_line >= end_line)
                .min_by_key(|s| s.end_line - s.start_line);

            if let Some(sym) = matching_sym {
                let prev_caller = self.current_caller.replace(sym.scope_path.clone());
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.visit(child);
                }
                self.current_caller = prev_caller;
                return;
            }
        }

        // Extract imports
        self.extract_import(node);

        // Extract call expressions
        self.extract_call(node);

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
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: target.trim().to_string(),
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                        });
                    }
                }
            }
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                if kind == "import_statement" {
                    if let Some(source_node) = node.child_by_field_name("source") {
                        let raw =
                            self.node_text(source_node).trim().trim_matches('"').trim_matches('\'');
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: raw.to_string(),
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                        });
                    }
                }
            }
            SupportedLanguage::Python => {
                if kind == "import_statement" || kind == "import_from_statement" {
                    let text = self.node_text(node).trim();
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: text.to_string(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                    });
                }
            }
            SupportedLanguage::Go => {
                if kind == "import_spec" {
                    let path = self.node_text(node).trim().trim_matches('"');
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: path.to_string(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                    });
                }
            }
            _ => {}
        }
    }

    fn extract_call(&mut self, node: Node) {
        let kind = node.kind();
        let is_call = kind == "call_expression"
            || kind == "method_call_expression"
            || kind == "invocation_expression"
            || kind == "call"
            || kind == "function_call";

        if !is_call {
            return;
        }

        let Some(ref caller) = self.current_caller else {
            return;
        };

        let callee_name = self.extract_callee_name(node);
        let Some(callee) = callee_name else {
            return;
        };

        if let Some(target_sym) = self.resolve_callee(&callee) {
            let key = (caller.clone(), target_sym.scope_path.clone());
            if !self.visited_calls.contains(&key) && caller != &target_sym.scope_path {
                self.visited_calls.insert(key);
                self.edges.push(Edge {
                    source: caller.clone(),
                    target: target_sym.scope_path.clone(),
                    edge_type: "calls".to_string(),
                    weight: 0.8,
                    provenance: EdgeProvenance::CodeCalls,
                });
            }
        }
    }

    fn extract_callee_name(&self, node: Node) -> Option<String> {
        let kind = node.kind();
        if kind == "call_expression" {
            let func = node.child_by_field_name("function")?;
            let func_kind = func.kind();
            if func_kind == "identifier" || func_kind == "property_identifier" {
                return Some(self.node_text(func).to_string());
            } else if func_kind == "field_expression" || func_kind == "member_expression" {
                if let Some(prop) = func
                    .child_by_field_name("field")
                    .or_else(|| func.child_by_field_name("property"))
                {
                    return Some(self.node_text(prop).to_string());
                }
            }
            Some(self.node_text(func).to_string())
        } else if kind == "method_call_expression" {
            let method = node.child_by_field_name("name")?;
            Some(self.node_text(method).to_string())
        } else {
            None
        }
    }

    fn resolve_callee(&self, callee_name: &str) -> Option<&'a CodeSymbol> {
        let clean_name = callee_name.rsplit("::").next().unwrap_or(callee_name);
        let clean_name = clean_name.rsplit('.').next().unwrap_or(clean_name);

        // 1. Search within the current file first (fastest and highest confidence)
        if let Some(local_match) = self.file_symbols.iter().find(|s| s.name == clean_name) {
            return Some(local_match);
        }

        // 2. Search in workspace symbols catalog
        if let Some(candidates) = self.symbol_index.get(clean_name) {
            if candidates.len() == 1 {
                return Some(candidates[0]);
            }
            // If multiple candidates, prioritize same directory / crate
            let file_dir = Path::new(&self.file_path).parent().unwrap_or_else(|| Path::new(""));
            if let Some(dir_match) = candidates.iter().find(|c| {
                Path::new(&c.file_path).parent().unwrap_or_else(|| Path::new("")) == file_dir
            }) {
                return Some(dir_match);
            }
            return candidates.first().copied();
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
}
