//! Tlaplus language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Tlaplus language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Tlaplus,
    name: "tlaplus",
    extensions: &["tla"],
    filenames: &[],
    grammar: || tree_sitter_tlaplus::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Tlaplus language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Tlaplus,
    function_node_kinds: &["operator_definition", "function_definition"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "\\*",
    doc_comment_kinds: &["comment", "block_comment"],
    callable_node_kinds: &["operator_definition", "function_definition"],
    import_node_kinds: &["extends", "instance"],
    call_node_kinds: &["bound_infix_op"],
};