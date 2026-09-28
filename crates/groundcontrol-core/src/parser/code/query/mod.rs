//! Tree-sitter `.scm` query pack engine for declarative symbol extraction.
//!
//! Replaces imperative, handwritten string slice matching with compiled
//! declarative Tree-sitter queries (`tags.scm`) for core languages.

use std::sync::OnceLock;

use groundcontrol_common::types::CodeSymbolType;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::parser::code::languages::SupportedLanguage;

const RUST_QUERY_SRC: &str = include_str!("packs/rust.scm");
const PYTHON_QUERY_SRC: &str = include_str!("packs/python.scm");
const TYPESCRIPT_QUERY_SRC: &str = include_str!("packs/typescript.scm");
const GO_QUERY_SRC: &str = include_str!("packs/go.scm");
const JAVA_QUERY_SRC: &str = include_str!("packs/java.scm");
const CSHARP_QUERY_SRC: &str = include_str!("packs/csharp.scm");
const C_QUERY_SRC: &str = include_str!("packs/c.scm");
const CPP_QUERY_SRC: &str = include_str!("packs/cpp.scm");
const RUBY_QUERY_SRC: &str = include_str!("packs/ruby.scm");
const PHP_QUERY_SRC: &str = include_str!("packs/php.scm");
const KOTLIN_QUERY_SRC: &str = include_str!("packs/kotlin.scm");
const SCALA_QUERY_SRC: &str = include_str!("packs/scala.scm");
const SWIFT_QUERY_SRC: &str = include_str!("packs/swift.scm");
const ELIXIR_QUERY_SRC: &str = include_str!("packs/elixir.scm");
const ERLANG_QUERY_SRC: &str = include_str!("packs/erlang.scm");

/// Compiled Tree-sitter query with pre-resolved capture indices for high-performance dispatch.
pub struct LanguageQuery {
    /// Underlying compiled Tree-sitter query.
    pub query: Query,
    /// Capture ID for `@name`.
    pub name_capture_id: Option<u32>,
    /// Capture ID for `@definition.function`.
    pub def_function_id: Option<u32>,
    /// Capture ID for `@definition.method`.
    pub def_method_id: Option<u32>,
    /// Capture ID for `@definition.class`.
    pub def_class_id: Option<u32>,
    /// Capture ID for `@definition.struct`.
    pub def_struct_id: Option<u32>,
    /// Capture ID for `@definition.interface`.
    pub def_interface_id: Option<u32>,
    /// Capture ID for `@definition.trait`.
    pub def_trait_id: Option<u32>,
    /// Capture ID for `@definition.enum`.
    pub def_enum_id: Option<u32>,
    /// Capture ID for `@definition.module`.
    pub def_module_id: Option<u32>,
    /// Capture ID for `@definition.type`.
    pub def_type_id: Option<u32>,
    /// Capture ID for `@doc`.
    pub doc_id: Option<u32>,
    /// Capture ID for `@inherits`.
    pub inherits_id: Option<u32>,
    /// Capture ID for `@extends`.
    pub extends_id: Option<u32>,
    /// Capture ID for `@implements`.
    pub implements_id: Option<u32>,
    /// Capture ID for `@implements_trait`.
    pub implements_trait_id: Option<u32>,
    /// Capture ID for `@test`.
    pub test_id: Option<u32>,
    /// Capture ID for `@definition.route`.
    pub def_route_id: Option<u32>,
}

impl LanguageQuery {
    /// Construct a new `LanguageQuery` and resolve standard capture IDs.
    pub fn new(query: Query) -> Self {
        let mut name_capture_id = None;
        let mut def_function_id = None;
        let mut def_method_id = None;
        let mut def_class_id = None;
        let mut def_struct_id = None;
        let mut def_interface_id = None;
        let mut def_trait_id = None;
        let mut def_enum_id = None;
        let mut def_module_id = None;
        let mut def_type_id = None;
        let mut doc_id = None;
        let mut inherits_id = None;
        let mut extends_id = None;
        let mut implements_id = None;
        let mut implements_trait_id = None;
        let mut test_id = None;
        let mut def_route_id = None;

        for (idx, name) in query.capture_names().iter().enumerate() {
            let id = idx as u32;
            match *name {
                "name" => name_capture_id = Some(id),
                "definition.function" => def_function_id = Some(id),
                "definition.method" => def_method_id = Some(id),
                "definition.class" => def_class_id = Some(id),
                "definition.struct" => def_struct_id = Some(id),
                "definition.interface" => def_interface_id = Some(id),
                "definition.trait" => def_trait_id = Some(id),
                "definition.enum" => def_enum_id = Some(id),
                "definition.module" => def_module_id = Some(id),
                "definition.type" => def_type_id = Some(id),
                "definition.route" => def_route_id = Some(id),
                "doc" => doc_id = Some(id),
                "inherits" => inherits_id = Some(id),
                "extends" => extends_id = Some(id),
                "implements" => implements_id = Some(id),
                "implements_trait" => implements_trait_id = Some(id),
                "test" => test_id = Some(id),
                _ => {}
            }
        }

        Self {
            query,
            name_capture_id,
            def_function_id,
            def_method_id,
            def_class_id,
            def_struct_id,
            def_interface_id,
            def_trait_id,
            def_enum_id,
            def_module_id,
            def_type_id,
            doc_id,
            inherits_id,
            extends_id,
            implements_id,
            implements_trait_id,
            test_id,
            def_route_id,
        }
    }

    /// Execute the compiled query over an AST root node, collecting deduplicated definitions.
    pub fn extract_matches<'a>(
        &self,
        root_node: Node<'a>,
        source: &'a [u8],
    ) -> Vec<ExtractedQueryDefinition<'a>> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root_node, source);
        let mut results_map: std::collections::HashMap<usize, ExtractedQueryDefinition<'a>> =
            std::collections::HashMap::new();

        while let Some(m) = matches.next() {
            let mut name_node = None;
            let mut def_node = None;
            let mut symbol_type = None;
            let mut inherits_nodes = Vec::new();
            let mut implements_nodes = Vec::new();
            let mut is_test = false;
            let mut dynamic_captures = Vec::new();

            for capture in m.captures() {
                let id = capture.index;
                if Some(id) == self.name_capture_id {
                    name_node = Some(capture.node);
                } else if Some(id) == self.def_function_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Function);
                } else if Some(id) == self.def_method_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Method);
                } else if Some(id) == self.def_class_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Class);
                } else if Some(id) == self.def_struct_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Struct);
                } else if Some(id) == self.def_interface_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Interface);
                } else if Some(id) == self.def_trait_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Trait);
                } else if Some(id) == self.def_enum_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Enum);
                } else if Some(id) == self.def_module_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Module);
                } else if Some(id) == self.def_type_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::TypeAlias);
                } else if Some(id) == self.def_route_id {
                    def_node = Some(capture.node);
                    symbol_type = Some(CodeSymbolType::Route);
                } else if Some(id) == self.inherits_id || Some(id) == self.extends_id {
                    inherits_nodes.push(capture.node);
                } else if Some(id) == self.implements_id || Some(id) == self.implements_trait_id {
                    implements_nodes.push(capture.node);
                } else if Some(id) == self.test_id {
                    is_test = true;
                } else {
                    let cap_name = self.query.capture_names()[id as usize];
                    dynamic_captures.push((cap_name.to_string(), capture.node));
                }
            }

            if let (Some(def), Some(name), Some(sym_type)) = (def_node, name_node, symbol_type) {
                let id = def.id();
                if let Some(existing) = results_map.get_mut(&id) {
                    if symbol_priority(sym_type) > symbol_priority(existing.symbol_type) {
                        existing.symbol_type = sym_type;
                        existing.name_node = name;
                    }
                    existing.inherits_nodes.extend(inherits_nodes);
                    existing.implements_nodes.extend(implements_nodes);
                    existing.dynamic_captures.extend(dynamic_captures);
                    existing.is_test |= is_test;
                } else {
                    results_map.insert(
                        id,
                        ExtractedQueryDefinition {
                            def_node: def,
                            name_node: name,
                            symbol_type: sym_type,
                            inherits_nodes,
                            implements_nodes,
                            is_test,
                            dynamic_captures,
                        },
                    );
                }
            }
        }

        // Deduplicate relations per definition
        for def in results_map.values_mut() {
            let mut seen_in = std::collections::HashSet::new();
            def.inherits_nodes.retain(|n| seen_in.insert(n.id()));
            let mut seen_im = std::collections::HashSet::new();
            def.implements_nodes.retain(|n| seen_im.insert(n.id()));
            let mut seen_dyn = std::collections::HashSet::new();
            def.dynamic_captures.retain(|(k, n)| seen_dyn.insert((k.clone(), n.id())));
        }

        results_map.into_values().collect()
    }
}

fn symbol_priority(sym: CodeSymbolType) -> u8 {
    match sym {
        CodeSymbolType::Class
        | CodeSymbolType::Struct
        | CodeSymbolType::Interface
        | CodeSymbolType::Trait
        | CodeSymbolType::Enum
        | CodeSymbolType::Module
        | CodeSymbolType::Method => 2,
        CodeSymbolType::Function => 1,
        CodeSymbolType::TypeAlias | CodeSymbolType::Constant => 0,
        CodeSymbolType::Route => 3,
    }
}

/// Extracted definition from declarative Tree-sitter query capture.
#[derive(Debug, Clone)]
pub struct ExtractedQueryDefinition<'a> {
    /// Outer AST node representing the definition boundary.
    pub def_node: Node<'a>,
    /// AST node representing the identifier / name of the symbol.
    pub name_node: Node<'a>,
    /// Syntactic category of the symbol.
    pub symbol_type: CodeSymbolType,
    /// Inherited class/type nodes from `@inherits` or `@extends`.
    pub inherits_nodes: Vec<Node<'a>>,
    /// Implemented interface/trait nodes from `@implements` or `@implements_trait`.
    pub implements_nodes: Vec<Node<'a>>,
    /// Whether the symbol is flagged as a test function (`@test`).
    pub is_test: bool,
    /// Dynamic relational captures for dynamic edge minting (`@rel`).
    pub dynamic_captures: Vec<(String, Node<'a>)>,
}

impl<'a> ExtractedQueryDefinition<'a> {
    /// First inherited node, if any.
    pub fn inherits_node(&self) -> Option<Node<'a>> {
        self.inherits_nodes.first().copied()
    }

    /// First implemented node, if any.
    pub fn implements_node(&self) -> Option<Node<'a>> {
        self.implements_nodes.first().copied()
    }
}

fn compile_query(lang: SupportedLanguage, source: &str) -> Option<LanguageQuery> {
    let ts_lang = lang.tree_sitter_language();
    match Query::new(&ts_lang, source) {
        Ok(q) => Some(LanguageQuery::new(q)),
        Err(err) => {
            tracing::error!(
                "Failed to compile tree-sitter query pack for {}: {:?}",
                lang.name(),
                err
            );
            None
        }
    }
}

/// Retrieve the pre-compiled `LanguageQuery` for a given language, if available.
pub fn get_language_query(lang: SupportedLanguage) -> Option<&'static LanguageQuery> {
    match lang {
        SupportedLanguage::Rust => {
            static RUST_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            RUST_Q.get_or_init(|| compile_query(lang, RUST_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Python => {
            static PY_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            PY_Q.get_or_init(|| compile_query(lang, PYTHON_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => {
            static TS_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            TS_Q.get_or_init(|| compile_query(lang, TYPESCRIPT_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Tsx => {
            static TSX_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            TSX_Q.get_or_init(|| compile_query(lang, TYPESCRIPT_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Go => {
            static GO_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            GO_Q.get_or_init(|| compile_query(lang, GO_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Java => {
            static JAVA_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            JAVA_Q.get_or_init(|| compile_query(lang, JAVA_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::CSharp => {
            static CS_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            CS_Q.get_or_init(|| compile_query(lang, CSHARP_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::C => {
            static C_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            C_Q.get_or_init(|| compile_query(lang, C_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Cpp => {
            static CPP_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            CPP_Q.get_or_init(|| compile_query(lang, CPP_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Ruby => {
            static RUBY_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            RUBY_Q.get_or_init(|| compile_query(lang, RUBY_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Php => {
            static PHP_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            PHP_Q.get_or_init(|| compile_query(lang, PHP_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Kotlin => {
            static KOTLIN_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            KOTLIN_Q.get_or_init(|| compile_query(lang, KOTLIN_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Scala => {
            static SCALA_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            SCALA_Q.get_or_init(|| compile_query(lang, SCALA_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Swift => {
            static SWIFT_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            SWIFT_Q.get_or_init(|| compile_query(lang, SWIFT_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Elixir => {
            static ELIXIR_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            ELIXIR_Q.get_or_init(|| compile_query(lang, ELIXIR_QUERY_SRC)).as_ref()
        }
        SupportedLanguage::Erlang => {
            static ERLANG_Q: OnceLock<Option<LanguageQuery>> = OnceLock::new();
            ERLANG_Q.get_or_init(|| compile_query(lang, ERLANG_QUERY_SRC)).as_ref()
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_query_packs_compile() {
        let languages = [
            SupportedLanguage::Rust,
            SupportedLanguage::Python,
            SupportedLanguage::TypeScript,
            SupportedLanguage::JavaScript,
            SupportedLanguage::Tsx,
            SupportedLanguage::Go,
            SupportedLanguage::Java,
            SupportedLanguage::CSharp,
            SupportedLanguage::C,
            SupportedLanguage::Cpp,
            SupportedLanguage::Ruby,
            SupportedLanguage::Php,
            SupportedLanguage::Kotlin,
            SupportedLanguage::Scala,
            SupportedLanguage::Swift,
            SupportedLanguage::Elixir,
            SupportedLanguage::Erlang,
        ];

        for lang in languages {
            let ts_lang = lang.tree_sitter_language();
            let src = match lang {
                SupportedLanguage::Rust => RUST_QUERY_SRC,
                SupportedLanguage::Python => PYTHON_QUERY_SRC,
                SupportedLanguage::TypeScript
                | SupportedLanguage::JavaScript
                | SupportedLanguage::Tsx => TYPESCRIPT_QUERY_SRC,
                SupportedLanguage::Go => GO_QUERY_SRC,
                SupportedLanguage::Java => JAVA_QUERY_SRC,
                SupportedLanguage::CSharp => CSHARP_QUERY_SRC,
                SupportedLanguage::C => C_QUERY_SRC,
                SupportedLanguage::Cpp => CPP_QUERY_SRC,
                SupportedLanguage::Ruby => RUBY_QUERY_SRC,
                SupportedLanguage::Php => PHP_QUERY_SRC,
                SupportedLanguage::Kotlin => KOTLIN_QUERY_SRC,
                SupportedLanguage::Scala => SCALA_QUERY_SRC,
                SupportedLanguage::Swift => SWIFT_QUERY_SRC,
                SupportedLanguage::Elixir => ELIXIR_QUERY_SRC,
                SupportedLanguage::Erlang => ERLANG_QUERY_SRC,
                _ => continue,
            };
            if let Err(e) = tree_sitter::Query::new(&ts_lang, src) {
                panic!("Query pack for language {} failed: {:?}", lang.name(), e);
            }
        }
    }
}
