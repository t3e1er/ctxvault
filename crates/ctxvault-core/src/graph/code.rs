//! Polyglot Code Graph Extractor & Lightweight Symbol/Import Resolver.
//!
//! Extracts structural AST relationships (`defines`, `imports`, `calls`, `implements_trait`)
//! across polyglot source code files and resolves cross-file call sites using SQLite
//! symbol catalogs and Petgraph.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ctxvault_common::types::{
    CodeSymbol, Edge, EdgeProvenance, ExternalRef, ExternalRefKind, ResolutionConfidence,
};
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

/// Result of extracting structural relationships from a single code file.
///
/// Carries the intra-repo structural edges (unchanged from prior behavior) plus
/// the [`ExternalRef`]s captured for call/import targets that did not resolve to
/// a local symbol. [`CodeExtraction::edges`] is byte-for-byte identical to the
/// edge set this extractor produced before external-reference capture was added;
/// external references are surfaced only via the separate
/// [`CodeExtraction::external_refs`] channel for a later cross-corpus
/// reconciliation pass and never alter the emitted `edges`.
#[derive(Debug, Clone, Default)]
pub struct CodeExtraction {
    /// Structural code edges (`defines`, `imports`, `calls`, `implements`, ...).
    pub edges: Vec<Edge>,
    /// Unresolved call/import targets ([`ExternalRefKind::Call`] /
    /// [`ExternalRefKind::Import`]) captured for later cross-corpus resolution.
    pub external_refs: Vec<ExternalRef>,
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
    /// using a pre-computed symbol index, alongside any unresolved external references.
    ///
    /// The returned [`CodeExtraction::edges`] is byte-for-byte identical to the edge set
    /// this extractor produced before external-reference capture was added; external references
    /// are surfaced only via the separate [`CodeExtraction::external_refs`] channel.
    pub fn extract_edges_for_file_with_index(
        file_path: &Path,
        content: &str,
        file_symbols: &[CodeSymbol],
        symbol_index: &HashMap<String, Vec<&CodeSymbol>>,
    ) -> CodeExtraction {
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
                target_path: None,
                target_symbol: None,
                target_kind: None,
            });
        }

        // 2. Parse AST for imports and call sites
        let Some(lang) = detect_language(file_path) else {
            return CodeExtraction { edges, external_refs: Vec::new() };
        };

        let mut parser = Parser::new();
        if parser.set_language(&lang.tree_sitter_language()).is_err() {
            return CodeExtraction { edges, external_refs: Vec::new() };
        }

        let Some(tree) = parser.parse(content, None) else {
            return CodeExtraction { edges, external_refs: Vec::new() };
        };

        let mut visitor =
            CallAndImportVisitor::new(file_path_str, content, lang, file_symbols, symbol_index);
        visitor.visit(tree.root_node());

        let mut external_refs = visitor.external_refs;
        let mut seen_imports = HashSet::new();
        // Import edges always target an out-of-corpus module path (they never resolve to an
        // in-corpus symbol), so each is an external reference. Derive them from the produced
        // edges so the edge Vec itself is left untouched.
        for e in &visitor.edges {
            if e.provenance == EdgeProvenance::CodeImports
                && seen_imports.insert((e.source.clone(), e.target.clone()))
            {
                external_refs.push(ExternalRef {
                    caller_scope_path: e.source.clone(),
                    raw_target: e.target.clone(),
                    kind: ExternalRefKind::Import,
                    confidence: ResolutionConfidence::Speculative,
                });
            }
        }

        edges.extend(visitor.edges);
        CodeExtraction { edges, external_refs }
    }

    /// Extract all structural edges (defines, imports, calls, implements) for a single code file.
    ///
    /// Returns only the intra-repo edge set; external-reference capture is a concern of the
    /// corpus-wide second pass, which calls [`Self::extract_edges_for_file_with_index`] directly.
    pub fn extract_edges_for_file(
        file_path: &Path,
        content: &str,
        file_symbols: &[CodeSymbol],
        all_symbols: &[CodeSymbol],
    ) -> Vec<Edge> {
        let symbol_index = Self::build_symbol_index(all_symbols);
        Self::extract_edges_for_file_with_index(file_path, content, file_symbols, &symbol_index)
            .edges
    }
}

struct CallAndImportVisitor<'a> {
    file_path: String,
    content: &'a str,
    language: SupportedLanguage,
    file_symbols: &'a [CodeSymbol],
    symbol_index: &'a HashMap<String, Vec<&'a CodeSymbol>>,
    current_caller: Option<String>,
    current_container: Option<String>,
    edges: Vec<Edge>,
    external_refs: Vec<ExternalRef>,
    visited_calls: HashSet<(String, String)>,
    visited_edges: HashSet<(String, String, String)>,
    visited_external_refs: HashSet<(String, String, ExternalRefKind)>,
    type_env: TypeEnvironment,
    depth: usize,
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
            current_container: None,
            edges: Vec::new(),
            external_refs: Vec::new(),
            visited_calls: HashSet::new(),
            visited_edges: HashSet::new(),
            visited_external_refs: HashSet::new(),
            type_env: TypeEnvironment::new(language),
            depth: 0,
        }
    }

    fn node_text(&self, node: Node) -> &str {
        &self.content[node.start_byte()..node.end_byte()]
    }

    fn add_rel_edge(
        &mut self,
        source: String,
        target: String,
        edge_type: &str,
        weight: f32,
        provenance: EdgeProvenance,
        confidence: ResolutionConfidence,
    ) {
        if source == target || source.is_empty() || target.is_empty() {
            return;
        }
        let key = (source.clone(), target.clone(), edge_type.to_string());
        if !self.visited_edges.contains(&key) {
            self.visited_edges.insert(key);
            self.edges.push(Edge {
                source,
                target,
                edge_type: edge_type.to_string(),
                weight,
                provenance,
                target_corpus: None,
                confidence: Some(confidence),
                target_path: None,
                target_symbol: None,
                target_kind: None,
            });
        }
    }

    /// Record an unresolved call/import target as an [`ExternalRef`], de-duped
    /// per `(caller_scope_path, raw_target, kind)` so re-visits do not duplicate.
    fn record_external_ref(&mut self, caller: String, raw_target: String, kind: ExternalRefKind) {
        if caller.is_empty() || raw_target.is_empty() {
            return;
        }
        let key = (caller.clone(), raw_target.clone(), kind);
        if self.visited_external_refs.insert(key) {
            self.external_refs.push(ExternalRef {
                caller_scope_path: caller,
                raw_target,
                kind,
                confidence: ResolutionConfidence::Speculative,
            });
        }
    }

    fn resolve_target(&self, raw_target: &str) -> (String, ResolutionConfidence) {
        let clean = raw_target.rsplit("::").next().unwrap_or(raw_target);
        let clean = clean.rsplit('.').next().unwrap_or(clean);

        if let Some(m) = self.file_symbols.iter().find(|s| s.name == clean) {
            return (m.scope_path.clone(), ResolutionConfidence::High);
        }

        if let Some(candidates) = self.symbol_index.get(clean) {
            if candidates.len() == 1 {
                return (candidates[0].scope_path.clone(), ResolutionConfidence::High);
            }
            let file_dir = Path::new(&self.file_path).parent().unwrap_or_else(|| Path::new(""));
            if let Some(dir_match) = candidates.iter().find(|c| {
                Path::new(&c.file_path).parent().unwrap_or_else(|| Path::new("")) == file_dir
            }) {
                return (dir_match.scope_path.clone(), ResolutionConfidence::Medium);
            }
            if let Some(first) = candidates.first() {
                return (first.scope_path.clone(), ResolutionConfidence::Speculative);
            }
        }

        (raw_target.to_string(), ResolutionConfidence::Speculative)
    }

    fn extract_name_from_descendants(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "object_reference" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    return Some(self.node_text(name_node).trim().to_string());
                }
                return Some(self.node_text(child).trim().to_string());
            }
        }
        None
    }

    const MAX_AST_DEPTH: usize = 256;

    fn visit(&mut self, node: Node) {
        if self.depth >= Self::MAX_AST_DEPTH {
            return;
        }
        self.depth += 1;
        self.visit_inner(node);
        self.depth -= 1;
    }

    fn visit_inner(&mut self, node: Node) {
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
            } else if let Some(n) = node.child_by_field_name("name") {
                Some(clean_type_name(self.node_text(n)))
            } else {
                self.extract_name_from_descendants(node)
            };

            let prev_container = self.current_container.take();
            self.current_container = container_name.clone().or_else(|| prev_container.clone());
            self.type_env.push_scope(container_name);

            // Container-level edge extractions
            self.extract_container_edges(node);

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                self.visit(child);
            }

            self.type_env.pop_scope();
            self.current_container = prev_container;
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

        // Language-specific AST relationship extractions:
        match self.language {
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                if kind == "decorator" {
                    self.extract_ts_decorator(node);
                }
            }
            SupportedLanguage::Python => {
                if kind == "decorated_definition" {
                    self.extract_python_decorated(node);
                }
            }
            SupportedLanguage::Rust => {
                if kind == "macro_invocation" {
                    self.extract_rust_macro(node);
                }
            }
            _ => {}
        }

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
                            target_path: None,
                            target_symbol: None,
                            target_kind: None,
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
                            target_path: None,
                            target_symbol: None,
                            target_kind: None,
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
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
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
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
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
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });
                }
            }
        }
    }

    fn extract_call(&mut self, node: Node) {
        let Some(caller) = self.current_caller.clone() else {
            return;
        };
        let caller = &caller;

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
                    target_path: None,
                    target_symbol: None,
                    target_kind: None,
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
                    target: target.clone(),
                    edge_type: "calls".to_string(),
                    weight: 0.5,
                    provenance: EdgeProvenance::CodeCalls,
                    target_corpus: None,
                    confidence: Some(ResolutionConfidence::Speculative),
                    target_path: None,
                    target_symbol: None,
                    target_kind: None,
                });
                // The callee did not resolve to any in-corpus symbol: capture it as an
                // external reference for later cross-corpus resolution.
                let caller = caller.clone();
                self.record_external_ref(caller, target, ExternalRefKind::Call);
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
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });
                }
            }
        }
    }

    fn extract_container_edges(&mut self, node: Node) {
        let kind = node.kind();
        let Some(container) = self.current_container.clone() else {
            return;
        };

        match self.language {
            SupportedLanguage::Rust => {
                if kind == "impl_item" {
                    if let Some(trait_node) = node.child_by_field_name("trait") {
                        let trait_name = self.node_text(trait_node).trim().to_string();
                        let (target, conf) = self.resolve_target(&trait_name);
                        self.add_rel_edge(
                            container.clone(),
                            target,
                            "implements",
                            0.9,
                            EdgeProvenance::CodeImplementsTrait,
                            conf,
                        );
                    }
                }
            }
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                if kind == "class_declaration" || kind == "class" {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "class_heritage" {
                            let mut hcursor = child.walk();
                            for hchild in child.children(&mut hcursor) {
                                if hchild.kind() == "extends_clause" {
                                    if let Some(val) = hchild.child_by_field_name("value") {
                                        let super_name = self.node_text(val).trim().to_string();
                                        let (target, conf) = self.resolve_target(&super_name);
                                        self.add_rel_edge(
                                            container.clone(),
                                            target,
                                            "extends",
                                            0.9,
                                            EdgeProvenance::CodeExtends,
                                            conf,
                                        );
                                    } else {
                                        for sc in hchild.children(&mut hchild.walk()) {
                                            let skind = sc.kind();
                                            if skind == "identifier" || skind == "type_identifier" {
                                                let super_name =
                                                    self.node_text(sc).trim().to_string();
                                                let (target, conf) =
                                                    self.resolve_target(&super_name);
                                                self.add_rel_edge(
                                                    container.clone(),
                                                    target,
                                                    "extends",
                                                    0.9,
                                                    EdgeProvenance::CodeExtends,
                                                    conf,
                                                );
                                            }
                                        }
                                    }
                                } else if hchild.kind() == "implements_clause" {
                                    let mut icursor = hchild.walk();
                                    for if_child in hchild.children(&mut icursor) {
                                        let ikind = if_child.kind();
                                        if ikind == "type_identifier" || ikind == "identifier" {
                                            let if_name =
                                                self.node_text(if_child).trim().to_string();
                                            let (target, conf) = self.resolve_target(&if_name);
                                            self.add_rel_edge(
                                                container.clone(),
                                                target,
                                                "implements",
                                                0.9,
                                                EdgeProvenance::CodeImplementsTrait,
                                                conf,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SupportedLanguage::Python => {
                if kind == "class_definition" {
                    if let Some(superclasses) = node.child_by_field_name("superclasses") {
                        let mut cursor = superclasses.walk();
                        for child in superclasses.children(&mut cursor) {
                            let ckind = child.kind();
                            if ckind == "identifier" || ckind == "attribute" {
                                let super_name = self.node_text(child).trim().to_string();
                                if !super_name.is_empty() {
                                    let (target, conf) = self.resolve_target(&super_name);
                                    self.add_rel_edge(
                                        container.clone(),
                                        target,
                                        "inherits",
                                        0.9,
                                        EdgeProvenance::CodeExtends,
                                        conf,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            SupportedLanguage::Go => {
                self.extract_go_embedded_fields(node, &container);
            }
            SupportedLanguage::Sql => {
                if kind == "create_table" || kind == "create_table_statement" {
                    self.extract_sql_foreign_key_references(node, &container);
                }
            }
            _ => {}
        }
    }

    fn extract_go_embedded_fields(&mut self, node: Node, container: &str) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "struct_type" {
                let mut fcursor = child.walk();
                for fchild in child.children(&mut fcursor) {
                    if fchild.kind() == "field_declaration_list" {
                        let mut dcursor = fchild.walk();
                        for field in fchild.children(&mut dcursor) {
                            if field.kind() == "field_declaration"
                                && field.child_by_field_name("name").is_none()
                            {
                                if let Some(type_node) = field.child_by_field_name("type") {
                                    let raw_type = self
                                        .node_text(type_node)
                                        .trim()
                                        .trim_start_matches('*')
                                        .trim()
                                        .to_string();
                                    if !raw_type.is_empty() {
                                        let (target, conf) = self.resolve_target(&raw_type);
                                        self.add_rel_edge(
                                            container.to_string(),
                                            target,
                                            "struct_embeds",
                                            0.85,
                                            EdgeProvenance::CodeStructEmbeds,
                                            conf,
                                        );
                                    }
                                } else {
                                    for c in field.children(&mut field.walk()) {
                                        let ckind = c.kind();
                                        if ckind == "type_identifier"
                                            || ckind == "qualified_type"
                                            || ckind == "pointer_type"
                                        {
                                            let raw_type = self
                                                .node_text(c)
                                                .trim()
                                                .trim_start_matches('*')
                                                .trim()
                                                .to_string();
                                            if !raw_type.is_empty() {
                                                let (target, conf) = self.resolve_target(&raw_type);
                                                self.add_rel_edge(
                                                    container.to_string(),
                                                    target,
                                                    "struct_embeds",
                                                    0.85,
                                                    EdgeProvenance::CodeStructEmbeds,
                                                    conf,
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                self.extract_go_embedded_fields(child, container);
            }
        }
    }

    fn extract_sql_foreign_key_references(&mut self, node: Node, container: &str) {
        let mut stack = vec![node];
        while let Some(curr) = stack.pop() {
            let kind = curr.kind();
            if kind == "keyword_references" || kind == "references_clause" || kind == "references" {
                let target_table = if let Some(next) = curr.next_named_sibling() {
                    if next.kind() == "object_reference" {
                        if let Some(n) = next.child_by_field_name("name") {
                            Some(self.node_text(n).trim().to_string())
                        } else {
                            Some(self.node_text(next).trim().to_string())
                        }
                    } else {
                        Some(self.node_text(next).trim().to_string())
                    }
                } else {
                    let mut found = None;
                    for c in curr.children(&mut curr.walk()) {
                        if c.kind() == "object_reference" {
                            found = Some(self.node_text(c).trim().to_string());
                            break;
                        }
                    }
                    found
                };

                if let Some(ref_table) = target_table {
                    let clean = ref_table.trim_matches('"').trim_matches('`').trim();
                    if !clean.is_empty() {
                        let (target, conf) = self.resolve_target(clean);
                        self.add_rel_edge(
                            container.to_string(),
                            target,
                            "foreign_key",
                            0.9,
                            EdgeProvenance::CodeForeignKey,
                            conf,
                        );
                    }
                }
            }

            let mut cursor = curr.walk();
            for child in curr.children(&mut cursor) {
                stack.push(child);
            }
        }
    }

    fn extract_ts_decorator(&mut self, node: Node) {
        let Some(name) = self.extract_decorator_name(node) else {
            return;
        };

        let decorated_target = if let Some(sibling) = node.next_named_sibling() {
            let skind = sibling.kind();
            if skind == "method_definition"
                || skind == "property_definition"
                || skind == "class_declaration"
                || skind == "function_declaration"
            {
                sibling.child_by_field_name("name").map(|n| {
                    let text = self.node_text(n).trim().to_string();
                    if let Some(ref cont) = self.current_container {
                        format!("{} > {}", cont, text)
                    } else {
                        text
                    }
                })
            } else {
                None
            }
        } else if let Some(parent) = node.parent() {
            if parent.kind() == "export_statement" {
                if let Some(decl) = parent.child_by_field_name("declaration") {
                    decl.child_by_field_name("name").map(|n| self.node_text(n).trim().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let target_sym = decorated_target
            .or_else(|| self.current_caller.clone())
            .or_else(|| self.current_container.clone());

        if let Some(sym) = target_sym {
            let (target, conf) = self.resolve_target(&name);
            self.add_rel_edge(sym, target, "decorates", 0.85, EdgeProvenance::CodeDecorates, conf);
        }
    }

    fn extract_decorator_name(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "call_expression" || kind == "call" {
                if let Some(func) = child.child_by_field_name("function").or_else(|| child.child(0))
                {
                    return Some(self.node_text(func).trim().to_string());
                }
            } else if kind == "identifier"
                || kind == "property_identifier"
                || kind == "attribute"
                || kind == "member_expression"
            {
                return Some(self.node_text(child).trim().to_string());
            }
        }
        None
    }

    fn extract_python_decorated(&mut self, node: Node) {
        let definition = node.child_by_field_name("definition").or_else(|| {
            let mut cursor = node.walk();
            let mut found = None;
            for c in node.children(&mut cursor) {
                let k = c.kind();
                if k == "function_definition" || k == "class_definition" {
                    found = Some(c);
                    break;
                }
            }
            found
        });

        let Some(def_node) = definition else {
            return;
        };
        let def_name =
            def_node.child_by_field_name("name").map(|n| self.node_text(n).trim().to_string());
        let Some(raw_name) = def_name else {
            return;
        };

        let full_scope = if let Some(ref cont) = self.current_container {
            format!("{} > {}", cont, raw_name)
        } else {
            raw_name
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "decorator" {
                if let Some(dec_name) = self.extract_decorator_name(child) {
                    let (target, conf) = self.resolve_target(&dec_name);
                    self.add_rel_edge(
                        full_scope.clone(),
                        target,
                        "decorates",
                        0.85,
                        EdgeProvenance::CodeDecorates,
                        conf,
                    );
                }
            }
        }
    }

    fn extract_rust_macro(&mut self, node: Node) {
        if let Some(macro_node) = node.child_by_field_name("macro").or_else(|| node.child(0)) {
            let macro_text =
                self.node_text(macro_node).trim().trim_end_matches('!').trim().to_string();
            if !macro_text.is_empty() {
                let caller = self
                    .current_caller
                    .clone()
                    .or_else(|| self.current_container.clone())
                    .unwrap_or_else(|| self.file_path.clone());

                let (target, conf) = self.resolve_target(&macro_text);
                self.add_rel_edge(
                    caller,
                    target,
                    "macro_expands",
                    0.7,
                    EdgeProvenance::CodeMacroExpands,
                    conf,
                );
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
    fn test_language_specific_ast_edges() {
        let config = ChunkingConfig::default();

        // 1. TypeScript: decorates, extends, implements
        let ts_code = r#"
@Injectable()
export class UserService extends BaseService implements IUserService {
    @Get('/users')
    getUsers() {}
}
"#;
        let ts_res =
            CodeChunker::parse_and_chunk(Path::new("src/user.ts"), ts_code, &config).unwrap();
        let ts_edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/user.ts"),
            ts_code,
            &ts_res.symbols,
            &ts_res.symbols,
        );
        assert!(
            ts_edges.iter().any(|e| e.edge_type == "extends"
                && e.source == "UserService"
                && e.target == "BaseService"),
            "Expected UserService -[:extends]-> BaseService, got: {:?}",
            ts_edges
        );
        assert!(
            ts_edges.iter().any(|e| e.edge_type == "implements"
                && e.source == "UserService"
                && e.target == "IUserService"),
            "Expected UserService -[:implements]-> IUserService, got: {:?}",
            ts_edges
        );
        assert!(
            ts_edges.iter().any(|e| e.edge_type == "decorates" && e.target == "Injectable"),
            "Expected decorates Injectable, got: {:?}",
            ts_edges
        );

        // 2. Python: decorates, inherits
        let py_code = r#"
@app.route("/items")
class ItemView(BaseView):
    @login_required
    def get(self):
        pass
"#;
        let py_res =
            CodeChunker::parse_and_chunk(Path::new("app/views.py"), py_code, &config).unwrap();
        let py_edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("app/views.py"),
            py_code,
            &py_res.symbols,
            &py_res.symbols,
        );
        assert!(
            py_edges.iter().any(|e| e.edge_type == "inherits"
                && e.source == "ItemView"
                && e.target == "BaseView"),
            "Expected ItemView -[:inherits]-> BaseView, got: {:?}",
            py_edges
        );
        assert!(
            py_edges.iter().any(|e| e.edge_type == "decorates" && e.target == "app.route"),
            "Expected decorates app.route, got: {:?}",
            py_edges
        );

        // 3. Rust: macro_expands
        let rs_code = r#"
pub fn run() {
    println!("hello");
    tokio::select! {
        _ = a => {}
    }
}
"#;
        let rs_res =
            CodeChunker::parse_and_chunk(Path::new("src/main.rs"), rs_code, &config).unwrap();
        let rs_edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("src/main.rs"),
            rs_code,
            &rs_res.symbols,
            &rs_res.symbols,
        );
        assert!(
            rs_edges.iter().any(|e| e.edge_type == "macro_expands" && e.target == "println"),
            "Expected run -[:macro_expands]-> println, got: {:?}",
            rs_edges
        );
        assert!(
            rs_edges.iter().any(|e| e.edge_type == "macro_expands" && e.target == "tokio::select"),
            "Expected run -[:macro_expands]-> tokio::select, got: {:?}",
            rs_edges
        );

        // 4. Go: struct_embeds
        let go_code = r#"
package server

import "sync"

type Server struct {
    sync.Mutex
    Logger
    port int
}
"#;
        let go_res =
            CodeChunker::parse_and_chunk(Path::new("server/server.go"), go_code, &config).unwrap();
        let go_edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("server/server.go"),
            go_code,
            &go_res.symbols,
            &go_res.symbols,
        );
        assert!(
            go_edges.iter().any(|e| e.edge_type == "struct_embeds" && e.target == "sync.Mutex"),
            "Expected Server -[:struct_embeds]-> sync.Mutex, got: {:?}",
            go_edges
        );
        assert!(
            go_edges.iter().any(|e| e.edge_type == "struct_embeds" && e.target == "Logger"),
            "Expected Server -[:struct_embeds]-> Logger, got: {:?}",
            go_edges
        );

        // 5. SQL: foreign_key
        let sql_code = r#"
CREATE TABLE orders (
    id INT PRIMARY KEY,
    user_id INT REFERENCES users(id),
    FOREIGN KEY (account_id) REFERENCES accounts(id)
);
"#;
        let sql_res =
            CodeChunker::parse_and_chunk(Path::new("schema/orders.sql"), sql_code, &config)
                .unwrap();
        let sql_edges = CodeGraphExtractor::extract_edges_for_file(
            Path::new("schema/orders.sql"),
            sql_code,
            &sql_res.symbols,
            &sql_res.symbols,
        );
        assert!(
            sql_edges.iter().any(|e| e.edge_type == "foreign_key" && e.target == "users"),
            "Expected orders -[:foreign_key]-> users, got: {:?}",
            sql_edges
        );
        assert!(
            sql_edges.iter().any(|e| e.edge_type == "foreign_key" && e.target == "accounts"),
            "Expected orders -[:foreign_key]-> accounts, got: {:?}",
            sql_edges
        );
    }

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

    #[test]
    fn test_external_ref_capture_local_vs_unresolved_call() {
        // The caller defines `run`, calls the locally-defined `helper` (resolves to a
        // real in-corpus symbol) and `missing_external` (resolves to nothing).
        let code = r#"
pub fn helper() {}

pub fn run() {
    helper();
    missing_external();
}
"#;
        let config = ChunkingConfig::default();
        let res = CodeChunker::parse_and_chunk(Path::new("src/lib.rs"), code, &config).unwrap();

        let symbol_index = CodeGraphExtractor::build_symbol_index(&res.symbols);
        let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
            Path::new("src/lib.rs"),
            code,
            &res.symbols,
            &symbol_index,
        );

        // (a) The fully-local call yields a normal `calls` edge...
        let local_edge = extraction
            .edges
            .iter()
            .find(|e| e.edge_type == "calls" && e.target == "helper")
            .expect("expected a resolved `calls` edge to the local helper");
        assert_eq!(local_edge.confidence, Some(ResolutionConfidence::High));

        // ...and NO external ref for the resolved local call.
        assert!(
            !extraction.external_refs.iter().any(|r| r.raw_target == "helper"),
            "a fully-local call must not produce an ExternalRef, got: {:?}",
            extraction.external_refs
        );

        // (b) The unresolved external call is captured as an ExternalRef.
        let ext = extraction
            .external_refs
            .iter()
            .find(|r| r.raw_target == "missing_external" && r.kind == ExternalRefKind::Call)
            .expect("expected an ExternalRef for the unresolved external call");
        assert_eq!(ext.caller_scope_path, "run");
        assert_eq!(ext.confidence, ResolutionConfidence::Speculative);
    }
}
