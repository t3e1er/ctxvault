//! Ruby language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Ruby language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Ruby,
    name: "ruby",
    extensions: &["rb", "rake", "gemspec"],
    filenames: &["gemfile", "rakefile"],
    grammar: || tree_sitter_ruby::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Ruby language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Ruby,
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
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["method", "singleton_method"],
    import_node_kinds: &["call"],
    call_node_kinds: &["call", "method_call"],
};