//! Proto language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Proto language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Proto,
    name: "proto",
    extensions: &["proto"],
    filenames: &[],
    grammar: || tree_sitter_proto::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Proto language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Proto,
    function_node_kinds: &["rpc"],
    method_node_kinds: &[],
    class_node_kinds: &["message", "service"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum"],
    module_node_kinds: &["package"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["rpc"],
    import_node_kinds: &["import"],
    call_node_kinds: &["rpc"],
};