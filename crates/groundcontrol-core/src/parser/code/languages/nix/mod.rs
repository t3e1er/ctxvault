//! Nix language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Nix language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Nix,
    name: "nix",
    extensions: &["nix"],
    filenames: &[],
    grammar: || tree_sitter_nix::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Nix language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Nix,
    function_node_kinds: &["function_expression", "binding"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["attrset_expression", "rec_attrset_expression"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_expression", "binding"],
    import_node_kinds: &[],
    call_node_kinds: &["apply_expression"],
};