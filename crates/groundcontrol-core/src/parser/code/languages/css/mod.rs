//! Css language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Css language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Css,
    name: "css",
    extensions: &["css"],
    filenames: &[],
    grammar: || tree_sitter_css::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Css language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Css,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &["rule_set", "media_statement", "keyframes_statement"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "/*",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["rule_set"],
    import_node_kinds: &["import_statement"],
    call_node_kinds: &["call_expression"],
};