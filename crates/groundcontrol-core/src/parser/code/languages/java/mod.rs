//! Java language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Java language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Java,
    name: "java",
    extensions: &["java"],
    filenames: &[],
    grammar: || tree_sitter_java::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Java language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Java,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("name"),
    comment_prefix: "//",
    doc_comment_kinds: &["line_comment", "block_comment"],
    callable_node_kinds: &["method_declaration", "constructor_declaration"],
    import_node_kinds: &["import_declaration"],
    call_node_kinds: &["method_invocation"],
};