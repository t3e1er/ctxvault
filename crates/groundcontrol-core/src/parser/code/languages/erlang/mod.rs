//! Erlang language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Erlang language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Erlang,
    name: "erlang",
    extensions: &["erl", "hrl"],
    filenames: &[],
    grammar: || tree_sitter_erlang::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Erlang language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Erlang,
    function_node_kinds: &["function_clause"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["record_decl"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module_attribute"],
    type_alias_node_kinds: &["type_alias"],
    name_field: Some("name"),
    comment_prefix: "%",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_clause"],
    import_node_kinds: &["attribute"],
    call_node_kinds: &["call"],
};