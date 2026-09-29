//! Julia language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Julia language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Julia,
    name: "julia",
    extensions: &["jl"],
    filenames: &[],
    grammar: || tree_sitter_julia::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Julia language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Julia,
    function_node_kinds: &["function_definition", "short_function_definition"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["struct_definition"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module_definition"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "#",
    doc_comment_kinds: &["line_comment", "block_comment"],
    callable_node_kinds: &["function_definition", "short_function_definition"],
    import_node_kinds: &["using_statement", "import_statement"],
    call_node_kinds: &["call_expression"],
};