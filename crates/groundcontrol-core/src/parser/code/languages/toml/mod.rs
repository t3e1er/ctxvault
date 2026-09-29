//! Toml language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Toml language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Toml,
    name: "toml",
    extensions: &["toml"],
    filenames: &[],
    grammar: || tree_sitter_toml_ng::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Toml language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Toml,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["table"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("key"),
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["table"],
    import_node_kinds: &[],
    call_node_kinds: &[],
};