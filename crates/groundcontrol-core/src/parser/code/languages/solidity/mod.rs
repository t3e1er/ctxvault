//! Solidity language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Solidity language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Solidity,
    name: "solidity",
    extensions: &["sol"],
    filenames: &[],
    grammar: || tree_sitter_solidity::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Solidity language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Solidity,
    function_node_kinds: &["function_definition"],
    method_node_kinds: &[],
    class_node_kinds: &["contract_declaration", "interface_declaration", "library_declaration"],
    struct_node_kinds: &["struct_declaration"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_declaration"],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_definition"],
    import_node_kinds: &["import_directive"],
    call_node_kinds: &["function_call"],
};