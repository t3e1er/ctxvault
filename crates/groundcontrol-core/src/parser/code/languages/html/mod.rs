//! Html language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Html language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Html,
    name: "html",
    extensions: &["html", "htm"],
    filenames: &[],
    grammar: || tree_sitter_html::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Html language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Html,
    function_node_kinds: &[],
    method_node_kinds: &[],
    class_node_kinds: &["element", "script_element", "style_element"],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: Some("tag_name"),
    comment_prefix: "<!--",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["element"],
    import_node_kinds: &[],
    call_node_kinds: &[],
};