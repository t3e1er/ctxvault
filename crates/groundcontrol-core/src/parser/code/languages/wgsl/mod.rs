//! Wgsl language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Wgsl language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Wgsl,
    name: "wgsl",
    extensions: &["wgsl"],
    filenames: &[],
    grammar: || tree_sitter_wgsl_bevy::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Wgsl language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Wgsl,
    function_node_kinds: &["function_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["struct_declaration"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &["type_alias_declaration"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_declaration"],
    import_node_kinds: &["enable_directive"],
    call_node_kinds: &["call_expression"],
};