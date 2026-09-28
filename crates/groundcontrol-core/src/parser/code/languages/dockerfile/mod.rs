//! Dockerfile language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Dockerfile language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Dockerfile,
    name: "dockerfile",
    extensions: &["dockerfile"],
    filenames: &["dockerfile", "containerfile"],
    grammar: || tree_sitter_containerfile::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Dockerfile language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Dockerfile,
    function_node_kinds: &["instruction", "run_instruction", "cmd_instruction", "entrypoint_instruction"],
    method_node_kinds: &[],
    class_node_kinds: &["from_instruction"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["from_instruction"],
    import_node_kinds: &["from_instruction"],
    call_node_kinds: &["run_instruction"],
};