//! Haskell language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Haskell language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Haskell,
    name: "haskell",
    extensions: &["hs", "lhs"],
    filenames: &[],
    grammar: || tree_sitter_haskell::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Haskell language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Haskell,
    function_node_kinds: &["function", "bind", "signature"],
    method_node_kinds: &[],
    class_node_kinds: &["class_declaration"],
    struct_node_kinds: &["data_declaration", "newtype_declaration"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module_declaration"],
    type_alias_node_kinds: &["type_synonym"],
    name_field: Some("name"),
    comment_prefix: "--",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function", "bind"],
    import_node_kinds: &["import"],
    call_node_kinds: &["apply"],
};