//! Make language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Make language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Make,
    name: "make",
    extensions: &["mk"],
    filenames: &["makefile", "gnumakefile"],
    grammar: || tree_sitter_make::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Make language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Make,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["rule"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["rule"],
    import_node_kinds: &["include_directive"],
    call_node_kinds: &["recipe"],
};