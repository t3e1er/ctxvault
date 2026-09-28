//! Php language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Php language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Php,
    name: "php",
    extensions: &["php", "phtml", "php3", "php4", "php5", "phps"],
    filenames: &[],
    grammar: || tree_sitter_php::LANGUAGE_PHP.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Php language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Php,
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
    callable_node_kinds: &["function_definition", "method_declaration"],
    import_node_kinds: &["namespace_use_declaration"],
    call_node_kinds: &["function_call_expression", "member_call_expression"],
};