//! Go language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Go language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Go,
    name: "go",
    extensions: &["go"],
    filenames: &[],
    grammar: || tree_sitter_go::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Go language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Go,
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
    callable_node_kinds: &["function_declaration", "method_declaration"],
    import_node_kinds: &["import_declaration", "import_spec"],
    call_node_kinds: &["call_expression"],
};