//! Rust language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Rust language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Rust,
    name: "rust",
    extensions: &["rs"],
    filenames: &[],
    grammar: || tree_sitter_rust::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Rust language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Rust,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["line_comment", "block_comment"],
    callable_node_kinds: &["function_item"],
    import_node_kinds: &["use_declaration"],
    call_node_kinds: &["call_expression", "method_call_expression"],
};