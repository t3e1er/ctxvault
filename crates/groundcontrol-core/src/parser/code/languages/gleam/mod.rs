//! Gleam language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Gleam language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Gleam,
    name: "gleam",
    extensions: &["gleam"],
    filenames: &[],
    grammar: || tree_sitter_gleam::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Gleam language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Gleam,
    function_node_kinds: &["function"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["type_definition"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &["type_alias"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function"],
    import_node_kinds: &["import"],
    call_node_kinds: &["function_call"],
};