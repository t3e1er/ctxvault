//! Sql language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Sql language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Sql,
    name: "sql",
    extensions: &["sql"],
    filenames: &[],
    grammar: || tree_sitter_sequel::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Sql language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Sql,
    function_node_kinds: &["create_function_statement", "create_procedure_statement"],
    method_node_kinds: &[],
    class_node_kinds: &["create_table_statement", "create_view_statement", "create_table", "create_view"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["create_schema_statement"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "--",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["create_function_statement", "create_procedure_statement"],
    import_node_kinds: &[],
    call_node_kinds: &["function_call"],
};