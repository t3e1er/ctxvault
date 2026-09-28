//! Tree-sitter `.scm` query pack engine for declarative symbol extraction.
//!
//! Replaces imperative, handwritten string slice matching with compiled
//! declarative Tree-sitter queries (`tags.scm`, `locals.scm`, `routes.scm`) for core languages.

pub mod cache;
pub use cache::get_language_query;

use groundcontrol_common::types::CodeSymbolType;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

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
    /// Capture ID for `@local.var`.
    pub local_var_id: Option<u32>,
    /// Capture ID for `@local.type`.
    pub local_type_id: Option<u32>,
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
        let mut local_var_id = None;
        let mut local_type_id = None;

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
                "local.var" => local_var_id = Some(id),
                "local.type" => local_type_id = Some(id),
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
            local_var_id,
            local_type_id,
        }
    }

    /// Execute the compiled query over an AST root node, collecting all matching symbol definitions.
    pub fn extract_symbols<'a>(
        &self,
        root_node: Node<'a>,
        source: &'a [u8],
    ) -> Vec<ExtractedQueryDefinition<'a>> {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root_node, source);
        let mut defs = Vec::new();

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
                let cname = self.query.capture_names()[id as usize];

                if Some(id) == self.name_capture_id {
                    name_node = Some(capture.node);
                } else if Some(id) == self.inherits_id || Some(id) == self.extends_id {
                    inherits_nodes.push(capture.node);
                } else if Some(id) == self.implements_id || Some(id) == self.implements_trait_id {
                    implements_nodes.push(capture.node);
                } else if Some(id) == self.test_id {
                    is_test = true;
                } else if let Some(rel) = cname.strip_prefix("rel.") {
                    dynamic_captures.push((rel.to_string(), capture.node));
                } else if matches!(
                    cname,
                    "handles"
                        | "decorates"
                        | "macro_expands"
                        | "foreign_key"
                        | "embeds_struct"
                        | "struct_embeds"
                ) {
                    dynamic_captures.push((cname.to_string(), capture.node));
                } else {
                    let st = if Some(id) == self.def_function_id {
                        Some(CodeSymbolType::Function)
                    } else if Some(id) == self.def_method_id {
                        Some(CodeSymbolType::Method)
                    } else if Some(id) == self.def_class_id {
                        Some(CodeSymbolType::Class)
                    } else if Some(id) == self.def_struct_id {
                        Some(CodeSymbolType::Struct)
                    } else if Some(id) == self.def_interface_id {
                        Some(CodeSymbolType::Interface)
                    } else if Some(id) == self.def_trait_id {
                        Some(CodeSymbolType::Trait)
                    } else if Some(id) == self.def_enum_id {
                        Some(CodeSymbolType::Enum)
                    } else if Some(id) == self.def_module_id {
                        Some(CodeSymbolType::Module)
                    } else if Some(id) == self.def_type_id {
                        Some(CodeSymbolType::TypeAlias)
                    } else if Some(id) == self.def_route_id {
                        Some(CodeSymbolType::Route)
                    } else {
                        None
                    };

                    if let Some(st) = st {
                        let should_replace = match symbol_type {
                            None => true,
                            Some(existing) => symbol_priority(st) > symbol_priority(existing),
                        };
                        if should_replace {
                            def_node = Some(capture.node);
                            symbol_type = Some(st);
                        }
                    }
                }
            }

            if let (Some(name), Some(def), Some(st)) = (name_node, def_node, symbol_type) {
                defs.push(ExtractedQueryDefinition {
                    def_node: def,
                    name_node: name,
                    symbol_type: st,
                    inherits_nodes,
                    implements_nodes,
                    is_test,
                    dynamic_captures,
                });
            }
        }

        defs
    }

    /// Execute the compiled query over an AST root node, collecting all matching symbol definitions.
    #[inline]
    pub fn extract_matches<'a>(
        &self,
        root_node: Node<'a>,
        source: &'a [u8],
    ) -> Vec<ExtractedQueryDefinition<'a>> {
        self.extract_symbols(root_node, source)
    }

    /// Execute the compiled query over an AST root node, collecting local variable-to-type bindings.
    pub fn extract_local_bindings<'a>(
        &self,
        root_node: Node<'a>,
        source: &'a [u8],
    ) -> Vec<ExtractedLocalBinding> {
        let (Some(var_id), Some(type_id)) = (self.local_var_id, self.local_type_id) else {
            return Vec::new();
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root_node, source);
        let mut bindings = Vec::new();

        while let Some(m) = matches.next() {
            let mut var_node = None;
            let mut type_node = None;

            for capture in m.captures() {
                if capture.index == var_id {
                    var_node = Some(capture.node);
                } else if capture.index == type_id {
                    type_node = Some(capture.node);
                }
            }

            if let (Some(var), Some(ty)) = (var_node, type_node) {
                let var_name = std::str::from_utf8(&source[var.start_byte()..var.end_byte()])
                    .unwrap_or_default()
                    .trim()
                    .trim_start_matches("mut ")
                    .to_string();
                let raw_type = std::str::from_utf8(&source[ty.start_byte()..ty.end_byte()])
                    .unwrap_or_default()
                    .trim();
                let type_name = crate::graph::hybrid_lsp::clean_type_name(raw_type);
                if !var_name.is_empty() && !type_name.is_empty() {
                    bindings.push(ExtractedLocalBinding {
                        var_name,
                        type_name,
                        byte_offset: var.start_byte(),
                        line: var.start_position().row + 1,
                    });
                }
            }
        }

        bindings
    }
}

/// Local variable-to-type binding extracted from Tree-sitter `@local.var` and `@local.type` captures.
#[derive(Debug, Clone)]
pub struct ExtractedLocalBinding {
    /// Variable identifier name (e.g. "client").
    pub var_name: String,
    /// Inferred or declared type name (e.g. "SearchClient").
    pub type_name: String,
    /// Start byte offset in the source buffer.
    pub byte_offset: usize,
    /// Line number (1-indexed).
    pub line: usize,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::code::languages::SupportedLanguage;

    #[test]
    fn test_all_query_packs_compile() {
        for &lang in SupportedLanguage::ALL {
            let query_src = lang.query_source();
            let ts_lang = lang.tree_sitter_language();
            if let Err(e) = tree_sitter::Query::new(&ts_lang, &query_src) {
                panic!("Query pack for language {} failed: {:?}", lang.name(), e);
            }

            // Also verify get_language_query resolves and compiles
            assert!(
                get_language_query(lang).is_some(),
                "LanguageQuery for {} should resolve",
                lang.name()
            );
        }
    }
}
