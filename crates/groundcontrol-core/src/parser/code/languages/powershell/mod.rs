//! PowerShell language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for PowerShell language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::PowerShell,
    name: "powershell",
    extensions: &["ps1", "psm1", "psd1"],
    filenames: &[],
    grammar: || tree_sitter_powershell::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for PowerShell language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::PowerShell,
    function_node_kinds: &["function_statement"],
    method_node_kinds: &[],
    class_node_kinds: &["class_statement"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &["enum_statement"],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["function_statement"],
    import_node_kinds: &["using_statement"],
    call_node_kinds: &["command", "invocable_expression"],
};