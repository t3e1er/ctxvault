//! Bicep language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Bicep language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Bicep,
    name: "bicep",
    extensions: &["bicep"],
    filenames: &[],
    grammar: || tree_sitter_bicep::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Bicep language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Bicep,
    function_node_kinds: &["parameter_declaration", "variable_declaration", "output_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["resource_declaration", "module_declaration"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["resource_declaration", "module_declaration"],
    import_node_kinds: &["import_declaration"],
    call_node_kinds: &["function_call"],
};