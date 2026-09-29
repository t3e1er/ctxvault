//! VB6 (Visual Basic 6.0) language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for VB6 language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Vb6,
    name: "vb6",
    extensions: &["bas", "cls", "frm", "ctl", "dob"],
    filenames: &[],
    grammar: || tree_sitter_vb6::language(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for VB6 language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Vb6,
    function_node_kinds: &["function_definition", "sub_definition"],
    method_node_kinds: &["property_definition"],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["source_file"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "'",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &[
        "sub_definition",
        "function_definition",
        "property_definition",
    ],
    import_node_kinds: &[],
    call_node_kinds: &["call_statement", "function_call"],
};
