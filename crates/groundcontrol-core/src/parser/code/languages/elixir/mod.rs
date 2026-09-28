//! Elixir language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Elixir language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Elixir,
    name: "elixir",
    extensions: &["ex", "exs"],
    filenames: &[],
    grammar: || tree_sitter_elixir::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Elixir language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Elixir,
    function_node_kinds: &["call"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["call"],
    import_node_kinds: &["call"],
    call_node_kinds: &["call"],
};