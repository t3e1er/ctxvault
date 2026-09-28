//! Dart language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Dart language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Dart,
    name: "dart",
    extensions: &["dart"],
    filenames: &[],
    grammar: || tree_sitter_dart::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Dart language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Dart,
    function_node_kinds: &["function_signature", "method_signature"],
    method_node_kinds: &[],
    class_node_kinds: &["class_definition"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &["mixin_declaration"],
    enum_node_kinds: &["enum_declaration"],
    module_node_kinds: &["library_directive"],
    type_alias_node_kinds: &["type_alias"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_signature", "method_signature"],
    import_node_kinds: &["import_or_export"],
    call_node_kinds: &["method_invocation", "function_expression_invocation"],
};