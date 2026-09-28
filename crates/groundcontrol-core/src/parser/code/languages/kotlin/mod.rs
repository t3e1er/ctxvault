//! Kotlin language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Kotlin language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Kotlin,
    name: "kotlin",
    extensions: &["kt", "kts"],
    filenames: &[],
    grammar: || tree_sitter_kotlin_ng::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Kotlin language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Kotlin,
    function_node_kinds: &["function_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &["class_declaration"],
    struct_node_kinds: &["object_declaration"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_entry"],
    module_node_kinds: &["package_header"],
    type_alias_node_kinds: &["type_alias"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["line_comment", "multiline_comment"],
    callable_node_kinds: &["function_declaration", "secondary_constructor"],
    import_node_kinds: &["import_header"],
    call_node_kinds: &["call_expression"],
};