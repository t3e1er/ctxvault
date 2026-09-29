//! Zig language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Zig language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Zig,
    name: "zig",
    extensions: &["zig"],
    filenames: &[],
    grammar: || tree_sitter_zig::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Zig language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Zig,
    function_node_kinds: &["fn_proto", "fn_decl"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["container_decl"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["line_comment", "doc_comment"],
    callable_node_kinds: &["fn_proto", "fn_decl"],
    import_node_kinds: &["builtin_call"],
    call_node_kinds: &["call_expression"],
};