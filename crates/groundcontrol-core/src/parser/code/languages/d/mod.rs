//! D language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for D language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::D,
    name: "d",
    extensions: &["d", "di"],
    filenames: &[],
    grammar: || tree_sitter_d::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for D language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::D,
    function_node_kinds: &["function_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &["class_declaration"],
    struct_node_kinds: &["struct_declaration"],
    interface_node_kinds: &["interface_declaration"],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_declaration"],
    module_node_kinds: &["module_declaration"],
    type_alias_node_kinds: &["alias_declaration"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_declaration"],
    import_node_kinds: &["import_declaration"],
    call_node_kinds: &["call_expression"],
};