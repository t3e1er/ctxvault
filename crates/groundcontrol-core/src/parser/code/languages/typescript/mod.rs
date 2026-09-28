//! TypeScript language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for TypeScript language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::TypeScript,
    name: "typescript",
    extensions: &["ts", "mts", "cts"],
    filenames: &[],
    grammar: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for TypeScript language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::TypeScript,
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
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_declaration", "method_definition", "function", "arrow_function"],
    import_node_kinds: &["import_statement"],
    call_node_kinds: &["call_expression", "new_expression"],
};