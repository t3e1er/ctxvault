//! Cmake language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Cmake language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Cmake,
    name: "cmake",
    extensions: &["cmake"],
    filenames: &["cmakelists.txt"],
    grammar: || tree_sitter_cmake::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Cmake language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Cmake,
    function_node_kinds: &["function_def", "macro_def"],
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
    doc_comment_kinds: &["line_comment", "bracket_comment"],
    callable_node_kinds: &["function_def", "macro_def"],
    import_node_kinds: &["normal_command"],
    call_node_kinds: &["normal_command"],
};