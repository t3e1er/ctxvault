//! COBOL language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for COBOL language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Cobol,
    name: "cobol",
    extensions: &["cbl", "cob", "cpy"],
    filenames: &[],
    grammar: || arborium_cobol::language().into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for COBOL language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Cobol,
    function_node_kinds: &["paragraph_header", "section_header"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["program_definition"],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "*>",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["paragraph_header", "section_header", "program_definition"],
    import_node_kinds: &["copy_statement"],
    call_node_kinds: &["call_statement"],
};
