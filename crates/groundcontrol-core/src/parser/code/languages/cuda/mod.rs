//! Cuda language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Cuda language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Cuda,
    name: "cuda",
    extensions: &["cu", "cuh"],
    filenames: &[],
    grammar: || tree_sitter_cuda::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Cuda language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Cuda,
    function_node_kinds: &["function_definition"],
    method_node_kinds: &[],
    class_node_kinds: &["class_specifier"],
    struct_node_kinds: &["struct_specifier"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_specifier"],
    module_node_kinds: &["namespace_definition"],
    type_alias_node_kinds: &["type_definition", "alias_declaration"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_definition"],
    import_node_kinds: &["preproc_include"],
    call_node_kinds: &["call_expression"],
};