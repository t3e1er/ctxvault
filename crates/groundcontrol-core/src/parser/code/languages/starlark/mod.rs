//! Starlark language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Starlark language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Starlark,
    name: "starlark",
    extensions: &["bzl", "star"],
    filenames: &["build", "build.bazel", "workspace", "workspace.bazel"],
    grammar: || tree_sitter_starlark::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Starlark language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Starlark,
    function_node_kinds: &["function_definition"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_definition"],
    import_node_kinds: &[],
    call_node_kinds: &["call"],
};