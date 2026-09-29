//! Scala language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Scala language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Scala,
    name: "scala",
    extensions: &["scala", "sc"],
    filenames: &[],
    grammar: || tree_sitter_scala::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Scala language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Scala,
    function_node_kinds: &["function_definition", "function_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &["class_definition"],
    struct_node_kinds: &["object_definition"],
    interface_node_kinds: &[],
    trait_node_kinds: &["trait_definition"],
    enum_node_kinds: &["enum_definition"],
    module_node_kinds: &["package_clause"],
    type_alias_node_kinds: &["type_definition"],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_definition"],
    import_node_kinds: &["import_declaration"],
    call_node_kinds: &["call_expression"],
};