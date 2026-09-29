//! Graphql language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Graphql language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Graphql,
    name: "graphql",
    extensions: &["graphql", "gql"],
    filenames: &[],
    grammar: || tree_sitter_graphql::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Graphql language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Graphql,
    function_node_kinds: &["field_definition"],
    method_node_kinds: &[],
    class_node_kinds: &["object_type_definition", "interface_type_definition", "schema_definition"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_type_definition"],
    module_node_kinds: &[],
    type_alias_node_kinds: &["union_type_definition"],
    name_field: Some("name"),
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["field_definition"],
    import_node_kinds: &[],
    call_node_kinds: &["field"],
};